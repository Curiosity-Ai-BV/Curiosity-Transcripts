import {
  CalendarBlank,
  CalendarPlus,
  CheckCircle,
  CopySimple,
  DownloadSimple,
  FileText,
  FolderOpen,
  MagnifyingGlass,
  Microphone,
  Moon,
  PencilSimple,
  ShieldCheck,
  Sun,
  Trash,
  WarningDiamond,
  Waveform,
} from "@phosphor-icons/react";
import { open } from "@tauri-apps/plugin-dialog";
import { useEffect, useMemo, useState } from "react";

import packageInfo from "../package.json";
import {
  DesktopCommandFacade,
  DesktopSnapshot,
  ExportFormat,
  exportFormatLabel,
  getMockDesktopSnapshot,
  mapAnalysisDisclosure,
  mapCommandJobState,
  mapDeleteState,
  mapExportState,
  mapLocalProcessingState,
  mapModelStatus,
  mapPermissionState,
  mapRawAudioRetention,
  mapRecordingState,
  mapTranscriptionState,
  PersistedRawAudioRetentionPolicy,
  searchMeetings,
  Tone,
} from "./commandAdapter";

import "./styles.css";

const appVersion = packageInfo.version;
const ACTIVE_JOB_POLL_INTERVAL_MS = 250;

interface AppProps {
  snapshot?: DesktopSnapshot;
  commandFacade?: DesktopCommandFacade;
  filePicker?: Partial<AppFilePicker>;
  clipboardWriter?: AppClipboardWriter;
}

interface AppFilePicker {
  chooseImportWavPath(): Promise<string | null>;
  chooseWhisperModelPath(): Promise<string | null>;
}

interface AppClipboardWriter {
  writeText(text: string): Promise<void>;
}

type PendingCommand =
  | "start"
  | "choose-wav"
  | "choose-whisper-model"
  | "import"
  | "stop"
  | "transcribe"
  | "rename"
  | "export"
  | "delete"
  | "summary"
  | "correct-segment"
  | "cancel-transcription"
  | "cancel-summary"
  | "test-whisper"
  | "test-ollama"
  | "save-whisper"
  | "save-analysis"
  | "save-retention"
  | "request-calendar"
  | "attach-calendar"
  | null;

type ThemeMode = "dark" | "light";

const defaultAppFilePicker: AppFilePicker = {
  chooseImportWavPath: chooseNativeImportWavPath,
  chooseWhisperModelPath: chooseNativeWhisperModelPath,
};

const defaultClipboardWriter: AppClipboardWriter = {
  async writeText(text: string) {
    const writeText = globalThis.navigator?.clipboard?.writeText;
    if (!writeText) {
      throw new Error("Clipboard API unavailable.");
    }
    await writeText.call(globalThis.navigator.clipboard, text);
  },
};

async function chooseNativeImportWavPath(): Promise<string | null> {
  const selected: string | string[] | null = await open({
    title: "Choose WAV audio file",
    multiple: false,
    directory: false,
    fileAccessMode: "scoped",
    filters: [
      {
        name: "WAV audio",
        extensions: ["wav"],
      },
    ],
  });

  if (Array.isArray(selected)) {
    return typeof selected[0] === "string" ? selected[0] : null;
  }

  return selected;
}

async function chooseNativeWhisperModelPath(): Promise<string | null> {
  const selected: string | string[] | null = await open({
    title: "Choose Whisper model file",
    multiple: false,
    directory: false,
    fileAccessMode: "scoped",
    filters: [
      {
        name: "Whisper model",
        extensions: ["bin", "gguf"],
      },
    ],
  });

  if (Array.isArray(selected)) {
    return typeof selected[0] === "string" ? selected[0] : null;
  }

  return selected;
}

interface SettingsFormState {
  whisperModelPath: string;
  ollamaBaseUrl: string;
  ollamaModel: string;
  rawAudioRetentionPolicy: PersistedRawAudioRetentionPolicy;
}

interface SettingsFeedback {
  tone: Tone;
  message: string;
  metadata?:
    | {
        kind: "whisper";
        fileSizeBytes: number;
        sha256: string;
      }
    | {
        kind: "ollama";
        selectedLocalModelTag: string | null;
        installedLocalModels: string[] | null;
        pullCommand: string | null;
      };
}

export default function App({ snapshot, commandFacade, filePicker, clipboardWriter }: AppProps) {
  const appFilePicker = { ...defaultAppFilePicker, ...filePicker };
  const appClipboardWriter = clipboardWriter ?? defaultClipboardWriter;
  const initialSnapshot = snapshot ?? getMockDesktopSnapshot();
  const [currentSnapshot, setCurrentSnapshot] = useState(initialSnapshot);
  const [query, setQuery] = useState("");
  const [connectedSearchResultIds, setConnectedSearchResultIds] = useState<string[] | null>(null);
  const [selectedMeetingId, setSelectedMeetingId] = useState(initialSnapshot.selectedMeetingId);
  const [renameTitle, setRenameTitle] = useState(selectedTitleFromSnapshot(initialSnapshot));
  const [recordingTitle, setRecordingTitle] = useState("");
  const [importWavPath, setImportWavPath] = useState("");
  const [editingSegmentId, setEditingSegmentId] = useState<string | null>(null);
  const [segmentDraft, setSegmentDraft] = useState("");
  const [settingsForm, setSettingsForm] = useState<SettingsFormState>(settingsFormFromSnapshot(initialSnapshot));
  const [settingsFeedback, setSettingsFeedback] = useState<SettingsFeedback | null>(null);
  const [pendingCommand, setPendingCommand] = useState<PendingCommand>(null);
  const [commandError, setCommandError] = useState<string | null>(null);
  const [selectedExportFormat, setSelectedExportFormat] = useState<ExportFormat>("json");
  const [theme, setTheme] = useState<ThemeMode>("dark");

  useEffect(() => {
    if (snapshot) {
      setCurrentSnapshot(snapshot);
      setSettingsForm(settingsFormFromSnapshot(snapshot));
      setConnectedSearchResultIds(null);
      setRenameTitle(selectedTitleFromSnapshot(snapshot));
      setEditingSegmentId(null);
      setSegmentDraft("");
      setSettingsFeedback(null);
      setCommandError(null);
      setPendingCommand(null);
    }
  }, [snapshot]);

  const commandUnavailable = currentSnapshot.commandSurface.detail;
  const commandSurfaceReady = Boolean(commandFacade && currentSnapshot.commandSurface.ready);
  const commandUnavailableTitle = commandSurfaceReady
    ? ""
    : commandFacade || commandUnavailable.startsWith("Preview shell")
      ? commandUnavailable || "Desktop command surface is unavailable."
      : "Desktop command surface is unavailable in this runtime.";
  const meetings = useMemo(() => {
    if (!commandSurfaceReady) {
      return searchMeetings(currentSnapshot.meetings, query);
    }
    if (!query.trim()) {
      return currentSnapshot.meetings;
    }
    if (!connectedSearchResultIds) {
      return [];
    }
    const resultIds = new Set(connectedSearchResultIds);
    return currentSnapshot.meetings.filter((meeting) => resultIds.has(meeting.id));
  }, [commandSurfaceReady, connectedSearchResultIds, currentSnapshot.meetings, query]);
  useEffect(() => {
    setSelectedMeetingId((current) => {
      return resolveSelectedMeetingId(currentSnapshot, current);
    });
  }, [currentSnapshot.meetings, currentSnapshot.selectedMeetingId]);

  const selectedMeeting = meetings.find((meeting) => meeting.id === selectedMeetingId) ?? meetings[0] ?? null;
  useEffect(() => {
    if (selectedMeeting) {
      setRenameTitle(selectedMeeting.title);
    } else {
      setRenameTitle("");
    }
    setEditingSegmentId(null);
    setSegmentDraft("");
  }, [selectedMeeting?.id, selectedMeeting?.title]);

  useEffect(() => {
    if (!commandFacade || !commandSurfaceReady) {
      setConnectedSearchResultIds(null);
      return;
    }
    const searchQuery = query.trim();
    if (!searchQuery) {
      setConnectedSearchResultIds(null);
      return;
    }

    let cancelled = false;
    commandFacade.searchMeetings({ query: searchQuery })
      .then((results) => {
        if (!cancelled) {
          setConnectedSearchResultIds(results.map((result) => result.meeting_id));
        }
      })
      .catch((error) => {
        if (!cancelled) {
          setConnectedSearchResultIds([]);
          setCommandError(commandErrorMessage(error));
        }
      });

    return () => {
      cancelled = true;
    };
  }, [commandFacade, commandSurfaceReady, query]);

  useEffect(() => {
    if (!commandFacade || !commandSurfaceReady || !snapshotHasActiveCommandJob(currentSnapshot)) {
      return;
    }

    let cancelled = false;
    const interval = window.setInterval(() => {
      commandFacade.desktopSnapshot()
        .then((nextSnapshot) => {
          if (!cancelled) {
            applyDesktopSnapshot(nextSnapshot);
          }
        })
        .catch((error) => {
          if (!cancelled) {
            setCommandError(commandErrorMessage(error));
          }
        });
    }, ACTIVE_JOB_POLL_INTERVAL_MS);

    return () => {
      cancelled = true;
      window.clearInterval(interval);
    };
  }, [
    commandFacade,
    commandSurfaceReady,
    currentSnapshot.transcriptionJob?.id,
    currentSnapshot.transcriptionJob?.state,
    currentSnapshot.summaryJob?.id,
    currentSnapshot.summaryJob?.state,
  ]);

  const isRecordingActive =
    currentSnapshot.recording.permission_state === "Ready" &&
    (currentSnapshot.recording.state === "Recording" || currentSnapshot.recording.state === "Paused");
  const commandBusy = pendingCommand !== null;
  const recording = commandUnavailable.startsWith("Preview shell")
    ? {
        label: "Recording unavailable",
        tone: "muted" as Tone,
        detail: commandUnavailable,
      }
    : mapRecordingState(currentSnapshot.recording);
  const model = mapModelStatus(currentSnapshot.model);
  const transcription = mapTranscriptionState(currentSnapshot.transcription);
  const transcriptionJob = currentSnapshot.transcriptionJob
    ? mapCommandJobState(currentSnapshot.transcriptionJob)
    : null;
  const summaryJob = currentSnapshot.summaryJob ? mapCommandJobState(currentSnapshot.summaryJob) : null;
  const setupGuidance = currentSnapshot.setupGuidance;
  const modelSetupOptions = currentSnapshot.modelSetupOptions;
  const calendarContext = currentSnapshot.calendarContext;
  const calendarTone = calendarContextTone(calendarContext);
  const whisperReadinessTone = whisperSetupTone(setupGuidance.whisper.state);
  const ollamaReadinessTone = ollamaSetupTone(setupGuidance.ollama);
  const whisperModelReady = currentSnapshot.model.kind === "ready";
  const startDisabled = !commandSurfaceReady || isRecordingActive || commandBusy;
  const stopDisabled = !commandSurfaceReady || !isRecordingActive || commandBusy;
  const transcribeDisabled = !commandSurfaceReady || !selectedMeeting || commandBusy || !whisperModelReady;
  const renameDisabled =
    !commandSurfaceReady ||
    !selectedMeeting ||
    commandBusy ||
    !renameTitle.trim() ||
    renameTitle.trim() === selectedMeeting.title;
  const exportDisabled = !commandSurfaceReady || !selectedMeeting || commandBusy;
  const selectedMeetingHasActiveDeleteBlockingJob =
    isSelectedActiveCommandJob(currentSnapshot.transcriptionJob, selectedMeeting?.id) ||
    isSelectedActiveCommandJob(currentSnapshot.summaryJob, selectedMeeting?.id);
  const deleteDisabled =
    !commandSurfaceReady || !selectedMeeting || commandBusy || selectedMeetingHasActiveDeleteBlockingJob;
  const recordingTitleDisabled = !commandSurfaceReady || isRecordingActive || commandBusy;
  const importWavPathDisabled = !commandSurfaceReady || isRecordingActive || commandBusy;
  const chooseWavDisabled = importWavPathDisabled;
  const importDisabled = importWavPathDisabled || !importWavPath.trim();
  const correctionDisabled =
    !commandSurfaceReady ||
    !selectedMeeting ||
    !editingSegmentId ||
    commandBusy ||
    !segmentDraft.trim();
  const settingsInputDisabled = commandBusy;
  const settingsActionDisabled = commandBusy;
  const chooseWhisperModelDisabled = !commandSurfaceReady || commandBusy;
  const requestCalendarDisabled =
    !commandSurfaceReady || commandBusy || calendarContext.permissionState !== "NotRequested";

  const exportState = selectedMeeting
    ? mapExportState(selectedMeeting.exportState)
    : mapExportState({ state: "idle" });
  const deleteState = selectedMeeting
    ? mapDeleteState(selectedMeeting.deleteState)
    : mapDeleteState({ state: "idle" });
  const rawAudioRetention = selectedMeeting
    ? mapRawAudioRetention(selectedMeeting.privacy.rawAudioRetention)
    : null;
  const localProcessingState = selectedMeeting
    ? mapLocalProcessingState(selectedMeeting.privacy.localOnly)
    : null;
  const exportCommandState = mapExportState(currentSnapshot.exportCommand);
  const deleteCommandState = mapDeleteState(currentSnapshot.deleteCommand);
  const failedDeleteMeetingId =
    currentSnapshot.deleteCommand.state === "failed" ? currentSnapshot.deleteCommand.meetingId?.trim() : undefined;
  const analysisDisclosure = selectedMeeting ? mapAnalysisDisclosure(selectedMeeting.analysis) : null;
  const selectedAnalysisCommand =
    selectedMeeting && currentSnapshot.analysisCommand?.meetingId === selectedMeeting.id
      ? currentSnapshot.analysisCommand
      : null;
  const summaryFailure = selectedAnalysisCommand?.state === "Failed" ? selectedAnalysisCommand.failure : null;
  const summaryDisabled =
    !commandSurfaceReady ||
    !selectedMeeting ||
    commandBusy ||
    selectedMeeting.segments.length === 0;
  const canCancelTranscriptionJob = isActiveCommandJob(currentSnapshot.transcriptionJob);
  const canCancelSummaryJob = isActiveCommandJob(currentSnapshot.summaryJob);
  const canRetryTranscriptionJob = isSelectedRetryableJob(currentSnapshot.transcriptionJob, selectedMeeting?.id);
  const canRetrySummaryJob = isSelectedRetryableJob(currentSnapshot.summaryJob, selectedMeeting?.id);
  const cancelTranscriptionDisabled =
    !commandSurfaceReady ||
    !currentSnapshot.transcriptionJob ||
    (commandBusy && pendingCommand !== "transcribe") ||
    !canCancelTranscriptionJob ||
    currentSnapshot.transcriptionJob.cancelRequested;
  const cancelSummaryDisabled =
    !commandSurfaceReady ||
    !currentSnapshot.summaryJob ||
    (commandBusy && pendingCommand !== "summary") ||
    !canCancelSummaryJob ||
    currentSnapshot.summaryJob.cancelRequested;
  const retryTranscriptionDisabled =
    !commandSurfaceReady ||
    !selectedMeeting ||
    !canRetryTranscriptionJob ||
    !whisperModelReady ||
    commandBusy;
  const retrySummaryDisabled =
    !commandSurfaceReady ||
    !selectedMeeting ||
    !canRetrySummaryJob ||
    selectedMeeting.segments.length === 0 ||
    commandBusy;

  async function runSnapshotCommand(
    pending: Exclude<PendingCommand, null>,
    command: (commands: DesktopCommandFacade) => Promise<DesktopSnapshot>,
  ) {
    if (
      !commandFacade ||
      !commandSurfaceReady ||
      (commandBusy && !commandAllowedDuringBusy(pending, pendingCommand))
    ) {
      return;
    }

    setPendingCommand(pending);
    setCommandError(null);
    try {
      const nextSnapshot = await command(commandFacade);
      applyDesktopSnapshot(nextSnapshot);
      setRecordingTitle("");
      if (pending === "import") {
        setImportWavPath("");
      }
    } catch (error) {
      setCommandError(commandErrorMessage(error));
    } finally {
      setPendingCommand(null);
    }
  }

  function applyDesktopSnapshot(nextSnapshot: DesktopSnapshot) {
    setCurrentSnapshot((current) => {
      const merged = preserveCommandJobProgress(current, nextSnapshot);
      setSelectedMeetingId((selected) => resolveSelectedMeetingId(merged, selected));
      return merged;
    });
  }

  function startRecording() {
    const title = recordingTitle.trim();
    void runSnapshotCommand(
      "start",
      (commands) => commands.startRecording(title ? { title } : undefined),
    );
  }

  function importWavFile() {
    const sourcePath = importWavPath.trim();
    if (!sourcePath) {
      return;
    }
    const title = recordingTitle.trim();
    void runSnapshotCommand(
      "import",
      (commands) => commands.importAudioFile(title ? { sourcePath, title } : { sourcePath }),
    );
  }

  async function chooseImportWavFile() {
    if (!commandSurfaceReady || isRecordingActive || commandBusy) {
      return;
    }

    setPendingCommand("choose-wav");
    setCommandError(null);
    try {
      const sourcePath = await appFilePicker.chooseImportWavPath();
      if (sourcePath) {
        setImportWavPath(sourcePath);
      }
    } catch (error) {
      setCommandError(commandErrorMessage(error));
    } finally {
      setPendingCommand(null);
    }
  }

  async function chooseWhisperModelFile() {
    if (!commandSurfaceReady || commandBusy) {
      return;
    }

    setPendingCommand("choose-whisper-model");
    setCommandError(null);
    setSettingsFeedback(null);
    try {
      const modelPath = await appFilePicker.chooseWhisperModelPath();
      if (modelPath) {
        setSettingsForm((current) => ({ ...current, whisperModelPath: modelPath }));
      }
    } catch (error) {
      setSettingsFeedback({ tone: "blocked", message: commandErrorMessage(error) });
    } finally {
      setPendingCommand(null);
    }
  }

  function stopRecording() {
    void runSnapshotCommand("stop", (commands) => commands.stopRecording());
  }

  function transcribeSelectedMeeting() {
    if (!selectedMeeting) {
      return;
    }
    void runSnapshotCommand("transcribe", (commands) =>
      commands.transcribeMeeting({ meetingId: selectedMeeting.id }),
    );
  }

  function renameSelectedMeeting() {
    if (!selectedMeeting) {
      return;
    }
    const title = renameTitle.trim();
    if (!title) {
      return;
    }
    void runSnapshotCommand("rename", (commands) =>
      commands.renameMeeting({ meetingId: selectedMeeting.id, title }),
    );
  }

  function exportSelectedMeeting() {
    if (!selectedMeeting) {
      return;
    }
    void runSnapshotCommand("export", (commands) =>
      commands.exportMeeting({ meetingId: selectedMeeting.id, format: selectedExportFormat }),
    );
  }

  function deleteSelectedMeeting() {
    if (!selectedMeeting) {
      return;
    }
    void runSnapshotCommand("delete", (commands) =>
      commands.deleteMeeting({ meetingId: selectedMeeting.id }),
    );
  }

  function generateSelectedSummary() {
    if (!selectedMeeting) {
      return;
    }
    void runSnapshotCommand("summary", (commands) =>
      commands.generateSummary({ meetingId: selectedMeeting.id }),
    );
  }

  function editTranscriptSegment(segmentId: string, text: string) {
    if (commandBusy) {
      return;
    }
    setCommandError(null);
    setEditingSegmentId(segmentId);
    setSegmentDraft(text);
  }

  function cancelTranscriptCorrection() {
    setEditingSegmentId(null);
    setSegmentDraft("");
  }

  function saveTranscriptCorrection() {
    if (!selectedMeeting || !editingSegmentId) {
      return;
    }
    const correctedText = segmentDraft.trim();
    if (!correctedText) {
      return;
    }
    const segmentId = editingSegmentId;
    const editedAtMs = Date.now();
    void runSnapshotCommand("correct-segment", async (commands) => {
      const nextSnapshot = await commands.correctTranscriptSegment({
        meetingId: selectedMeeting.id,
        segmentId,
        correctedText,
        editedAtMs,
      });
      setEditingSegmentId(null);
      setSegmentDraft("");
      return nextSnapshot;
    });
  }

  function cancelTranscriptionJob() {
    const job = currentSnapshot.transcriptionJob;
    if (!job) {
      return;
    }
    void runSnapshotCommand("cancel-transcription", (commands) =>
      commands.cancelTranscription({ jobId: job.id }),
    );
  }

  function cancelSummaryJob() {
    const job = currentSnapshot.summaryJob;
    if (!job) {
      return;
    }
    void runSnapshotCommand("cancel-summary", (commands) => commands.cancelSummary({ jobId: job.id }));
  }

  function retryFailedDelete() {
    if (!failedDeleteMeetingId) {
      return;
    }
    void runSnapshotCommand("delete", (commands) =>
      commands.deleteMeeting({ meetingId: failedDeleteMeetingId }),
    );
  }

  async function runSettingsSnapshotCommand(
    pending: Exclude<PendingCommand, null>,
    command: (commands: DesktopCommandFacade) => Promise<DesktopSnapshot>,
    successMessage: string,
  ) {
    if (commandBusy) {
      return;
    }
    if (!commandFacade || !commandSurfaceReady) {
      setSettingsFeedback({ tone: "blocked", message: commandUnavailableTitle });
      return;
    }

    setPendingCommand(pending);
    setCommandError(null);
    setSettingsFeedback(null);
    try {
      const nextSnapshot = await command(commandFacade);
      setCurrentSnapshot(nextSnapshot);
      setSelectedMeetingId((current) => resolveSelectedMeetingId(nextSnapshot, current));
      setSettingsForm(settingsFormFromSnapshot(nextSnapshot));
      setSettingsFeedback({ tone: "ready", message: successMessage });
    } catch (error) {
      setSettingsFeedback({ tone: "blocked", message: commandErrorMessage(error) });
    } finally {
      setPendingCommand(null);
    }
  }

  async function testWhisperModelPath() {
    if (commandBusy) {
      return;
    }
    if (!commandFacade || !commandSurfaceReady) {
      setSettingsFeedback({ tone: "blocked", message: commandUnavailableTitle });
      return;
    }

    setPendingCommand("test-whisper");
    setCommandError(null);
    setSettingsFeedback(null);
    try {
      const testedPath = settingsForm.whisperModelPath;
      const result = await commandFacade.testWhisperModelPath({ path: testedPath });
      const testedPathTrimmed = testedPath.trim();
      const savedWhisperPath = currentSnapshot.settings.whisperModelPath.trim();
      const effectiveWhisperPath = currentSnapshot.model.configuredPath.trim();
      if (
        result.state === "Valid" &&
        testedPathTrimmed !== "" &&
        (testedPathTrimmed === savedWhisperPath || testedPathTrimmed === effectiveWhisperPath)
      ) {
        const nextSnapshot = await commandFacade.desktopSnapshot();
        applyDesktopSnapshot(nextSnapshot);
      }
      setSettingsFeedback({
        tone: result.state === "Valid" ? "ready" : "blocked",
        message: result.message || result.setupGuidance,
        metadata:
          result.state === "Valid"
            ? {
                kind: "whisper",
                fileSizeBytes: result.fileSizeBytes,
                sha256: result.sha256,
              }
            : undefined,
      });
    } catch (error) {
      setSettingsFeedback({ tone: "blocked", message: commandErrorMessage(error) });
    } finally {
      setPendingCommand(null);
    }
  }

  async function testOllamaConnection() {
    if (commandBusy) {
      return;
    }
    if (!commandFacade || !commandSurfaceReady) {
      setSettingsFeedback({ tone: "blocked", message: commandUnavailableTitle });
      return;
    }

    setPendingCommand("test-ollama");
    setCommandError(null);
    setSettingsFeedback(null);
    try {
      const testedBaseUrl = settingsForm.ollamaBaseUrl;
      const testedModel = settingsForm.ollamaModel;
      const result = await commandFacade.testOllamaConnection({
        baseUrl: testedBaseUrl,
        model: testedModel,
      });
      if (
        testedBaseUrl.trim() === currentSnapshot.settings.ollamaBaseUrl.trim() &&
        testedModel.trim() === currentSnapshot.settings.ollamaModel.trim()
      ) {
        const nextSnapshot = await commandFacade.desktopSnapshot();
        applyDesktopSnapshot(nextSnapshot);
      }
      setSettingsFeedback({
        tone: result.state === "Available" ? "ready" : "blocked",
        message: result.message || result.setupGuidance,
        metadata: {
          kind: "ollama",
          selectedLocalModelTag: result.selectedLocalModelTag,
          installedLocalModels: result.installedLocalModels,
          pullCommand: result.pullCommand,
        },
      });
    } catch (error) {
      setSettingsFeedback({ tone: "blocked", message: commandErrorMessage(error) });
    } finally {
      setPendingCommand(null);
    }
  }

  function updateOllamaBaseUrl(value: string) {
    setSettingsForm((current) => ({ ...current, ollamaBaseUrl: value }));
    setSettingsFeedback(null);
  }

  function updateOllamaModel(value: string) {
    setSettingsForm((current) => ({ ...current, ollamaModel: value }));
    setSettingsFeedback(null);
  }

  function chooseOllamaCandidate(modelTag: string) {
    setSettingsForm((current) => ({ ...current, ollamaModel: modelTag }));
    setSettingsFeedback(null);
  }

  async function copyOllamaPullCommand(pullCommand: string) {
    if (commandBusy) {
      return;
    }

    try {
      await appClipboardWriter.writeText(pullCommand);
      setSettingsFeedback({
        tone: "ready",
        message: "Pull command copied.",
        metadata: {
          kind: "ollama",
          selectedLocalModelTag: null,
          installedLocalModels: null,
          pullCommand,
        },
      });
    } catch (error) {
      setSettingsFeedback({
        tone: "blocked",
        message: `Could not copy pull command: ${commandErrorMessage(error)}`,
        metadata: {
          kind: "ollama",
          selectedLocalModelTag: null,
          installedLocalModels: null,
          pullCommand,
        },
      });
    }
  }

  function updateRawAudioRetentionPolicy(value: PersistedRawAudioRetentionPolicy) {
    setSettingsForm((current) => ({ ...current, rawAudioRetentionPolicy: value }));
    setSettingsFeedback(null);
  }

  function saveWhisperModelPath() {
    void runSettingsSnapshotCommand(
      "save-whisper",
      (commands) =>
        commands.saveWhisperModelPath({ whisperModelPath: settingsForm.whisperModelPath }),
      "Whisper model path saved.",
    );
  }

  function saveAnalysisSettings() {
    void runSettingsSnapshotCommand(
      "save-analysis",
      (commands) =>
        commands.saveAnalysisSettings({
          ollamaBaseUrl: settingsForm.ollamaBaseUrl,
          ollamaModel: settingsForm.ollamaModel,
        }),
      "Analysis settings saved.",
    );
  }

  function saveRawAudioRetentionPolicy() {
    void runSettingsSnapshotCommand(
      "save-retention",
      (commands) =>
        commands.saveRawAudioRetentionPolicy({
          rawAudioRetentionPolicy: settingsForm.rawAudioRetentionPolicy,
        }),
      "Raw-audio retention saved.",
    );
  }

  function requestCalendarAccess() {
    void runSettingsSnapshotCommand(
      "request-calendar",
      (commands) => commands.requestAppleCalendarAccess(),
      "Calendar permission state refreshed.",
    );
  }

  function attachCalendarEvent(event: DesktopSnapshot["calendarContext"]["upcomingEvents"][number]) {
    if (!selectedMeeting) {
      return;
    }
    void runSnapshotCommand("attach-calendar", (commands) =>
      commands.attachCalendarEventContext({
        meetingId: selectedMeeting.id,
        eventId: event.id,
        privacyConfirmed: event.privacy === "Unknown",
      }),
    );
  }

  const busyCommandTitle = "A desktop command is already running.";
  const startButtonTitle = !commandSurfaceReady
    ? commandUnavailableTitle
      : commandBusy
        ? busyCommandTitle
        : isRecordingActive
          ? "Stop the active recording before starting another one."
        : "Start desktop recording.";
  const stopButtonTitle = !commandSurfaceReady
    ? commandUnavailableTitle
      : commandBusy
        ? busyCommandTitle
        : isRecordingActive
        ? "Stop desktop recording."
        : "No active desktop recording to stop.";
  const importButtonTitle = !commandSurfaceReady
    ? commandUnavailableTitle
    : commandBusy
      ? busyCommandTitle
      : isRecordingActive
        ? "Stop the active recording before importing audio."
        : importWavPath.trim()
          ? "Import the WAV file into private app storage."
          : "Enter a local WAV source path before importing.";
  const chooseWavButtonTitle = !commandSurfaceReady
    ? commandUnavailableTitle
    : commandBusy
      ? busyCommandTitle
      : isRecordingActive
        ? "Stop the active recording before choosing audio."
        : "Choose a local WAV source file.";
  const chooseWhisperModelButtonTitle = !commandSurfaceReady
    ? commandUnavailableTitle
    : commandBusy
      ? busyCommandTitle
      : "Choose a local Whisper model file.";
  const transcribeButtonTitle = !commandSurfaceReady
    ? commandUnavailableTitle
    : commandBusy
      ? busyCommandTitle
      : !selectedMeeting
        ? "Select a meeting before transcription."
        : currentSnapshot.model.kind === "missing"
          ? "Choose a local Whisper model file before transcription."
          : currentSnapshot.model.kind === "unsupported"
            ? "Choose a supported .bin or .gguf Whisper model file before transcription."
          : currentSnapshot.model.kind === "untested"
            ? "Run Test path for the saved Whisper model file before transcription."
            : "Transcribe the selected meeting with the configured local Whisper model.";
  const retryTranscriptionButtonTitle = !commandSurfaceReady
    ? commandUnavailableTitle
    : commandBusy
      ? busyCommandTitle
      : !whisperModelReady
        ? transcribeButtonTitle
        : "Retry transcription for the selected meeting.";
  const renameButtonTitle = !commandSurfaceReady
    ? commandUnavailableTitle
    : commandBusy
      ? busyCommandTitle
      : selectedMeeting
        ? "Rename the selected meeting."
        : "Select a meeting before renaming.";
  const exportButtonTitle = !commandSurfaceReady
    ? commandUnavailableTitle
    : commandBusy
      ? busyCommandTitle
      : selectedMeeting
        ? `Export the selected meeting as ${exportFormatLabel(selectedExportFormat)}.`
        : "Select a meeting before exporting.";
  const selectedExportFormatLabel = exportFormatLabel(selectedExportFormat);
  const deleteButtonTitle = !commandSurfaceReady
    ? commandUnavailableTitle
    : commandBusy
      ? busyCommandTitle
      : selectedMeetingHasActiveDeleteBlockingJob
        ? "Cancel or wait for the active transcription or summary job before deleting private data."
      : selectedMeeting
        ? "Delete app-private data for the selected meeting."
        : "Select a meeting before deleting private data.";
  const summaryButtonTitle = !commandSurfaceReady
    ? commandUnavailableTitle
    : commandBusy
      ? busyCommandTitle
      : !selectedMeeting
        ? "Select a meeting before requesting a summary."
        : selectedMeeting.segments.length === 0
          ? "Generate a transcript before requesting a summary."
          : "Generate a local Ollama summary for the selected meeting.";
  const isLightTheme = theme === "light";
  const themeButtonLabel = isLightTheme ? "Switch to dark mode" : "Switch to light mode";

  function toggleTheme() {
    setTheme((current) => (current === "dark" ? "light" : "dark"));
  }

  const recordingControls = (
    <section className="recording-strip" aria-label="Recording controls and status">
      <div className="strip-primary">
        <IconFrame tone={recording.tone}>
          <Waveform size={22} weight="regular" />
        </IconFrame>
        <div>
          <div className="strip-heading-row">
            <h2>Recording</h2>
            <StatusPill tone={recording.tone} label={recording.label} />
          </div>
          <p>{recording.detail}</p>
        </div>
      </div>
      <div className="recording-actions">
        <label className="recording-title-field" htmlFor="recording-title">
          <span>Recording title</span>
          <input
            id="recording-title"
            value={recordingTitle}
            onChange={(event) => setRecordingTitle(event.target.value)}
            placeholder="Optional meeting title"
            disabled={recordingTitleDisabled}
          />
        </label>
        <div className="import-wav-control">
          <label className="recording-title-field" htmlFor="import-wav-path">
            <span>WAV source path</span>
            <input
              id="import-wav-path"
              value={importWavPath}
              onChange={(event) => setImportWavPath(event.target.value)}
              placeholder="/path/to/audio.wav"
              disabled={importWavPathDisabled}
            />
          </label>
          <button
            type="button"
            className="button"
            disabled={chooseWavDisabled}
            title={chooseWavButtonTitle}
            onClick={chooseImportWavFile}
          >
            <FolderOpen size={16} weight="regular" />
            {pendingCommand === "choose-wav" ? "Choosing WAV" : "Choose WAV"}
          </button>
        </div>
        <div className="recording-buttons">
          <button
            type="button"
            className="button primary"
            disabled={startDisabled}
            title={startButtonTitle}
            onClick={startRecording}
          >
            <Microphone size={16} weight="regular" />
            {pendingCommand === "start" ? "Starting recording" : "Start recording"}
          </button>
          <button
            type="button"
            className="button"
            disabled={importDisabled}
            title={importButtonTitle}
            onClick={importWavFile}
          >
            <FileText size={16} weight="regular" />
            {pendingCommand === "import" ? "Importing WAV" : "Import WAV"}
          </button>
          <button
            type="button"
            className="button"
            disabled={stopDisabled}
            title={stopButtonTitle}
            onClick={stopRecording}
          >
            <Waveform size={16} weight="regular" />
            {pendingCommand === "stop" ? "Stopping recording" : "Stop recording"}
          </button>
        </div>
        <span className="recording-path">{currentSnapshot.recording.storage_location.app_private_path}</span>
      </div>
    </section>
  );

  return (
    <main className="app-shell" data-theme={theme}>
      <section className="workspace" aria-label="Transcript workspace">
        <header className="topbar">
          <div className="brand-lockup">
            <span className="brand-mark" aria-hidden="true">
              <Waveform size={22} weight="fill" />
            </span>
            <div>
              <p className="eyebrow">Curiosity Transcripts</p>
              <h1>Transcript workspace</h1>
            </div>
          </div>
          <div className="topbar-controls" aria-label="Workspace controls">
            <span className="version-badge" aria-label={`Version ${appVersion}`}>
              v{appVersion}
            </span>
            <button
              type="button"
              className="theme-toggle"
              aria-label={themeButtonLabel}
              aria-pressed={isLightTheme}
              title={themeButtonLabel}
              onClick={toggleTheme}
            >
              {isLightTheme ? <Moon size={16} weight="regular" /> : <Sun size={16} weight="regular" />}
              <span>{isLightTheme ? "Dark" : "Light"}</span>
            </button>
          </div>
        </header>

        {commandError ? (
          <p role="alert" className="command-error">
            {commandError}
          </p>
        ) : null}
        {currentSnapshot.exportCommand.state !== "idle" ? (
          <p role="status" className={`command-outcome ${exportCommandState.tone}`}>
            <strong>{exportCommandState.label}</strong>
            <span>{exportCommandState.detail}</span>
          </p>
        ) : null}
        {currentSnapshot.deleteCommand.state !== "idle" ? (
          <div role="status" className={`command-outcome ${deleteCommandState.tone}`}>
            <strong>{deleteCommandState.label}</strong>
            <span>{deleteCommandState.detail}</span>
            {failedDeleteMeetingId ? (
              <button
                type="button"
                className="button danger"
                disabled={!commandSurfaceReady || commandBusy}
                title={commandBusy ? busyCommandTitle : "Retry deletion for the failed meeting."}
                onClick={retryFailedDelete}
              >
                <Trash size={16} weight="regular" />
                Retry delete
              </button>
            ) : null}
          </div>
        ) : null}
        {summaryFailure ? (
          <div role="status" className="command-outcome blocked">
            <strong>{summaryFailure.message}</strong>
            {summaryFailure.setupGuidance ? <span>{summaryFailure.setupGuidance}</span> : null}
          </div>
        ) : null}

        <div className="content-grid">
          <aside className="meeting-pane" aria-label="Meetings">
            <div className="pane-heading">
              <p className="eyebrow">History</p>
              <h2>Meetings</h2>
            </div>
            <div className="search-block">
              <label htmlFor="meeting-search">Search meetings</label>
              <div className="search-control">
                <MagnifyingGlass size={16} weight="regular" />
                <input
                  id="meeting-search"
                  value={query}
                  onChange={(event) => setQuery(event.target.value)}
                  placeholder="Title or transcript text"
                />
              </div>
            </div>

            {currentSnapshot.loading ? <SkeletonList /> : null}

            <div className="meeting-list">
              {meetings.map((meeting) => (
                <button
                  type="button"
                  key={meeting.id}
                  className={meeting.id === selectedMeeting?.id ? "meeting-row selected" : "meeting-row"}
                  aria-pressed={meeting.id === selectedMeeting?.id}
                  aria-current={meeting.id === selectedMeeting?.id ? "page" : undefined}
                  onClick={() => setSelectedMeetingId(meeting.id)}
                >
                  <span className="meeting-title">{meeting.title}</span>
                  <span className="meeting-meta">
                    {meeting.startedAt} / {meeting.duration}
                  </span>
                  <span className="meeting-state">{meeting.transcriptState}</span>
                </button>
              ))}
            </div>

            {!currentSnapshot.loading && query && meetings.length === 0 ? <p className="empty-state">No meetings match this search.</p> : null}
          </aside>

          <section className="detail-pane" aria-label="Meeting detail">
            {selectedMeeting ? (
              <>
                {recordingControls}

                <div className="detail-header">
                  <div>
                    <p className="eyebrow">{selectedMeeting.startedAt}</p>
                    <h2>{selectedMeeting.title}</h2>
                    <div className="rename-title-row">
                      <label className="rename-title-field" htmlFor="selected-meeting-title">
                        <span>Selected meeting title</span>
                        <input
                          id="selected-meeting-title"
                          value={renameTitle}
                          onChange={(event) => setRenameTitle(event.target.value)}
                          disabled={!commandSurfaceReady || commandBusy}
                        />
                      </label>
                      <button
                        type="button"
                        className="button"
                        disabled={renameDisabled}
                        title={renameButtonTitle}
                        onClick={renameSelectedMeeting}
                      >
                        <PencilSimple size={16} weight="regular" />
                        {pendingCommand === "rename" ? "Renaming" : "Rename"}
                      </button>
                    </div>
                  </div>
                  <div className="detail-header-actions">
                    <StatusPill tone={selectedMeeting.transcriptState === "Ready" ? "ready" : "active"} label={selectedMeeting.transcriptState} />
                    <button
                      type="button"
                      className="button primary"
                      disabled={transcribeDisabled}
                      title={transcribeButtonTitle}
                      onClick={transcribeSelectedMeeting}
                    >
                      <FileText size={16} weight="regular" />
                      {pendingCommand === "transcribe" ? "Transcribing" : "Transcribe"}
                    </button>
                  </div>
                </div>

                <div className="privacy-row" aria-label="Meeting privacy data state">
                  <StatusLine icon={<ShieldCheck size={18} weight="regular" />} label={selectedMeeting.privacy.storageLabel} value={selectedMeeting.privacy.storagePath} tone="ready" />
                  {rawAudioRetention ? (
                    <StatusLine icon={<Waveform size={18} weight="regular" />} label={rawAudioRetention.label} value={rawAudioRetention.detail} tone={rawAudioRetention.tone} />
                  ) : null}
                  {localProcessingState ? (
                    <StatusLine icon={<ShieldCheck size={18} weight="regular" />} label={localProcessingState.label} value={localProcessingState.detail} tone={localProcessingState.tone} />
                  ) : null}
                  {selectedMeeting.calendarAttachment ? (
                    <StatusLine
                      icon={<CalendarBlank size={18} weight="regular" />}
                      label="Calendar context"
                      value={formatMeetingCalendarAttachment(selectedMeeting.calendarAttachment)}
                      tone="ready"
                    />
                  ) : null}
                  <StatusLine icon={<FileText size={18} weight="regular" />} label={exportState.label} value={exportState.detail} tone={exportState.tone} />
                  <StatusLine icon={<Trash size={18} weight="regular" />} label={deleteState.label} value={deleteState.detail} tone={deleteState.tone} />
                </div>

                {analysisDisclosure ? (
                  <section className="summary-section" aria-label="Structured summary">
                    <div>
                      <h3>Structured summary</h3>
                      <StatusLine
                        icon={<ShieldCheck size={18} weight="regular" />}
                        label={analysisDisclosure.label}
                        value={analysisDisclosure.detail}
                        tone={analysisDisclosure.tone}
                      />
                      {selectedMeeting.analysis?.summary ? (
                        <p className="summary-text">{selectedMeeting.analysis.summary}</p>
                      ) : null}
                    </div>
                    <button
                      type="button"
                      className="button"
                      disabled={summaryDisabled}
                      title={summaryButtonTitle}
                      onClick={generateSelectedSummary}
                    >
                      <FileText size={16} weight="regular" />
                      {pendingCommand === "summary" ? "Generating summary" : "Generate summary"}
                    </button>
                  </section>
                ) : null}

                <section className="transcript-section">
                  <h3>Transcript</h3>
                  <div className="segments">
                    {selectedMeeting.segments.map((segment) => {
                      const isEditingSegment = editingSegmentId === segment.id;
                      const showOriginalText = Boolean(
                        segment.originalText && segment.originalText !== segment.text,
                      );

                      return (
                        <article key={segment.id} className="segment">
                          <time>{formatTime(segment.startMs)}</time>
                          <div className="segment-body">
                            {isEditingSegment ? (
                              <div className="segment-editor">
                                <label className="segment-editor-field">
                                  <span>Transcript segment text</span>
                                  <textarea
                                    value={segmentDraft}
                                    onChange={(event) => setSegmentDraft(event.target.value)}
                                    disabled={pendingCommand === "correct-segment"}
                                  />
                                </label>
                                <div className="segment-editor-actions">
                                  <button
                                    type="button"
                                    className="button primary"
                                    disabled={correctionDisabled}
                                    title={
                                      commandSurfaceReady
                                        ? "Save the user correction for this transcript segment."
                                        : commandUnavailableTitle
                                    }
                                    onClick={saveTranscriptCorrection}
                                  >
                                    <CheckCircle size={16} weight="regular" />
                                    {pendingCommand === "correct-segment" ? "Saving correction" : "Save correction"}
                                  </button>
                                  <button
                                    type="button"
                                    className="button quiet"
                                    disabled={pendingCommand === "correct-segment"}
                                    onClick={cancelTranscriptCorrection}
                                  >
                                    Cancel correction
                                  </button>
                                </div>
                              </div>
                            ) : (
                              <>
                                <p>{segment.text}</p>
                                {showOriginalText ? (
                                  <small className="segment-original">Original: {segment.originalText}</small>
                                ) : null}
                              </>
                            )}
                          </div>
                          <span className="segment-channel">{segment.sourceChannel}</span>
                          {isEditingSegment ? null : (
                            <button
                              type="button"
                              className="button quiet segment-edit-button"
                              disabled={!commandSurfaceReady || commandBusy}
                              title={
                                commandSurfaceReady
                                  ? "Edit this transcript segment."
                                  : commandUnavailableTitle
                              }
                              onClick={() => editTranscriptSegment(segment.id, segment.text)}
                            >
                              <PencilSimple size={16} weight="regular" />
                              Edit segment
                            </button>
                          )}
                        </article>
                      );
                    })}
                  </div>
                </section>

                <div className="detail-actions">
                  <label className="export-format-field">
                    <span>Export format</span>
                    <select
                      value={selectedExportFormat}
                      disabled={exportDisabled}
                      onChange={(event) => setSelectedExportFormat(event.target.value as ExportFormat)}
                    >
                      <option value="json">JSON</option>
                      <option value="markdown">Markdown</option>
                      <option value="srt">SRT</option>
                    </select>
                  </label>
                  <button
                    type="button"
                    className="button"
                    disabled={exportDisabled}
                    title={exportButtonTitle}
                    onClick={exportSelectedMeeting}
                  >
                    <DownloadSimple size={16} weight="regular" />
                    {pendingCommand === "export"
                      ? `Exporting ${selectedExportFormatLabel}`
                      : `Export ${selectedExportFormatLabel}`}
                  </button>
                  <button
                    type="button"
                    className="button danger"
                    disabled={deleteDisabled}
                    title={deleteButtonTitle}
                    onClick={deleteSelectedMeeting}
                  >
                    <Trash size={16} weight="regular" />
                    {pendingCommand === "delete" ? "Deleting private data" : "Delete private data"}
                  </button>
                </div>
              </>
            ) : (
              <>
                {recordingControls}
                <p className="empty-state">No meeting selected.</p>
              </>
            )}
          </section>

          <aside className="settings-pane" aria-label="Settings and model status">
            <div className="pane-heading">
              <p className="eyebrow">Processing engine</p>
              <h2>Settings</h2>
            </div>
            <div className="engine-stack" aria-label="Model and capture status">
              <StatusLine icon={<CheckCircle size={18} weight="regular" />} label={model.label} value={model.detail} tone={model.tone} />
              <StatusLine icon={<FileText size={18} weight="regular" />} label={transcription.label} value={transcription.detail} tone={transcription.tone} />
              {transcriptionJob ? (
                <>
                  <StatusLine
                    icon={<FileText size={18} weight="regular" />}
                    label={transcriptionJob.label}
                    value={transcriptionJob.detail}
                    tone={transcriptionJob.tone}
                  />
                  {canCancelTranscriptionJob ? (
                    <button
                      type="button"
                      className="button"
                      disabled={cancelTranscriptionDisabled}
                      title={
                        commandSurfaceReady
                          ? "Request cancellation for the active transcription job."
                          : commandUnavailableTitle
                      }
                      onClick={cancelTranscriptionJob}
                    >
                      {pendingCommand === "cancel-transcription" ? "Canceling transcription" : "Cancel transcription"}
                    </button>
                  ) : null}
                  {canRetryTranscriptionJob ? (
                    <button
                      type="button"
                      className="button"
                      disabled={retryTranscriptionDisabled}
                      title={retryTranscriptionButtonTitle}
                      onClick={transcribeSelectedMeeting}
                    >
                      {pendingCommand === "transcribe" ? "Retrying transcription" : "Retry transcription"}
                    </button>
                  ) : null}
                </>
              ) : null}
              {summaryJob ? (
                <>
                  <StatusLine
                    icon={<FileText size={18} weight="regular" />}
                    label={summaryJob.label}
                    value={summaryJob.detail}
                    tone={summaryJob.tone}
                  />
                  {canCancelSummaryJob ? (
                    <button
                      type="button"
                      className="button"
                      disabled={cancelSummaryDisabled}
                      title={
                        commandSurfaceReady
                          ? "Request cancellation for the active summary job."
                          : commandUnavailableTitle
                      }
                      onClick={cancelSummaryJob}
                    >
                      {pendingCommand === "cancel-summary" ? "Canceling summary" : "Cancel summary"}
                    </button>
                  ) : null}
                  {canRetrySummaryJob ? (
                    <button
                      type="button"
                      className="button"
                      disabled={retrySummaryDisabled}
                      title={summaryButtonTitle}
                      onClick={generateSelectedSummary}
                    >
                      {pendingCommand === "summary" ? "Retrying summary" : "Retry summary"}
                    </button>
                  ) : null}
                </>
              ) : null}
              <StatusLine
                icon={<Microphone size={18} weight="regular" />}
                label={captureLabel(currentSnapshot.capture.microphone)}
                value={captureDetail(currentSnapshot.capture.microphone)}
                tone={captureTone(currentSnapshot.capture.microphone)}
              />
              <StatusLine
                icon={<WarningDiamond size={18} weight="regular" />}
                label={captureLabel(currentSnapshot.capture.systemAudio)}
                value={captureDetail(currentSnapshot.capture.systemAudio)}
                tone={captureTone(currentSnapshot.capture.systemAudio)}
              />
              <StatusLine
                icon={<CalendarBlank size={18} weight="regular" />}
                label={calendarContextLabel(calendarContext)}
                value={calendarContext.message}
                tone={calendarTone}
              />
              {selectedMeeting ? (
                <StatusLine
                  icon={<ShieldCheck size={18} weight="regular" />}
                  label={analysisDisclosure?.label ?? "Summary unavailable"}
                  value={analysisDisclosure?.detail ?? "No selected meeting."}
                  tone={analysisDisclosure?.tone ?? "muted"}
                />
              ) : null}
            </div>
            <div className="model-readiness" aria-label="Model readiness guidance">
              <div className={`readiness-item ${whisperReadinessTone}`}>
                <div className="readiness-heading">
                  <StatusPill tone={whisperReadinessTone} label={whisperSetupLabel(setupGuidance.whisper.state)} />
                </div>
                <p>{setupGuidance.whisper.message}</p>
                {setupGuidance.whisper.configuredPath ? (
                  <span className="readiness-path">{setupGuidance.whisper.configuredPath}</span>
                ) : null}
                <p>{setupGuidance.whisper.setupGuidance}</p>
                <small>{setupGuidance.whisper.compatibilityNote}</small>
                {setupGuidance.whisper.lastPathTest ? (
                  <div className="readiness-evidence">
                    <strong>
                      Last explicit Test path: {setupGuidance.whisper.lastPathTest.state} at{" "}
                      {formatEvidenceTimestamp(setupGuidance.whisper.lastPathTest.testedAtMs)}
                    </strong>
                    <span>Tested path: {setupGuidance.whisper.lastPathTest.testedPath || "none"}</span>
                    {setupGuidance.whisper.lastPathTest.fileSizeBytes !== null ? (
                      <span>Size: {setupGuidance.whisper.lastPathTest.fileSizeBytes} bytes</span>
                    ) : null}
                    {setupGuidance.whisper.lastPathTest.sha256 ? (
                      <span>SHA-256: {setupGuidance.whisper.lastPathTest.sha256}</span>
                    ) : null}
                    {setupGuidance.whisper.lastPathTest.failureDetail ? (
                      <span>{setupGuidance.whisper.lastPathTest.failureDetail}</span>
                    ) : null}
                  </div>
                ) : null}
                {setupGuidance.whisper.lastSuccessfulTranscription ? (
                  <div className="readiness-evidence">
                    <strong>
                      Last successful transcription at{" "}
                      {formatEvidenceTimestamp(setupGuidance.whisper.lastSuccessfulTranscription.usedAtMs)}
                    </strong>
                    <span>Model path: {setupGuidance.whisper.lastSuccessfulTranscription.modelPath}</span>
                    <span>Provider: {setupGuidance.whisper.lastSuccessfulTranscription.provider}</span>
                    <span>Model: {setupGuidance.whisper.lastSuccessfulTranscription.modelName}</span>
                    <span>Meeting: {setupGuidance.whisper.lastSuccessfulTranscription.meetingId}</span>
                    <span>Model run: {setupGuidance.whisper.lastSuccessfulTranscription.modelRunId}</span>
                    <span>Transcript version: {setupGuidance.whisper.lastSuccessfulTranscription.transcriptVersionId}</span>
                    <span>
                      Transcript: {setupGuidance.whisper.lastSuccessfulTranscription.segmentCount} segment
                      {setupGuidance.whisper.lastSuccessfulTranscription.segmentCount === 1 ? "" : "s"}
                    </span>
                    <span>Model file size: {setupGuidance.whisper.lastSuccessfulTranscription.fileSizeBytes} bytes</span>
                    <span>
                      Model modified:{" "}
                      {formatEvidenceTimestamp(setupGuidance.whisper.lastSuccessfulTranscription.modifiedAtMs)}
                    </span>
                  </div>
                ) : null}
              </div>
              <div className={`readiness-item ${ollamaReadinessTone}`}>
                <div className="readiness-heading">
                  <StatusPill tone={ollamaReadinessTone} label={ollamaSetupLabel(setupGuidance.ollama)} />
                </div>
                <p>{setupGuidance.ollama.message}</p>
                <span className="readiness-path">
                  {setupGuidance.ollama.baseUrl} / {setupGuidance.ollama.model}
                </span>
                <p>{setupGuidance.ollama.setupGuidance}</p>
                {setupGuidance.ollama.lastConnectionTest ? (
                  <div className="readiness-evidence">
                    <strong>
                      Last explicit Test Ollama: {setupGuidance.ollama.lastConnectionTest.state} at{" "}
                      {formatEvidenceTimestamp(setupGuidance.ollama.lastConnectionTest.testedAtMs)}
                    </strong>
                    <span>
                      Request: {setupGuidance.ollama.lastConnectionTest.baseUrl} /{" "}
                      {setupGuidance.ollama.lastConnectionTest.requestedModel}
                    </span>
                    {setupGuidance.ollama.lastConnectionTest.selectedLocalModelTag ? (
                      <span>Selected model: {setupGuidance.ollama.lastConnectionTest.selectedLocalModelTag}</span>
                    ) : null}
                    {setupGuidance.ollama.lastConnectionTest.installedLocalModels ? (
                      <span>
                        Observed models:{" "}
                        {setupGuidance.ollama.lastConnectionTest.installedLocalModels.length > 0
                          ? setupGuidance.ollama.lastConnectionTest.installedLocalModels.join(", ")
                          : "none reported"}
                      </span>
                    ) : null}
                    {setupGuidance.ollama.lastConnectionTest.pullCommand ? (
                      <span className="pull-command-copy">
                        <span>Pull command: {setupGuidance.ollama.lastConnectionTest.pullCommand}</span>
                        <CopyPullCommandButton
                          pullCommand={setupGuidance.ollama.lastConnectionTest.pullCommand}
                          disabled={commandBusy}
                          onCopy={copyOllamaPullCommand}
                        />
                      </span>
                    ) : null}
                    {setupGuidance.ollama.lastConnectionTest.failureDetail ? (
                      <span>{setupGuidance.ollama.lastConnectionTest.failureDetail}</span>
                    ) : null}
                    <small>Last explicit observation, not current availability.</small>
                  </div>
                ) : null}
              </div>
            </div>
            <div className="model-setup-options" aria-label="Manual model setup options">
              <div className="setup-option-group">
                <strong>{modelSetupOptions.whisper.title}</strong>
                <p>{modelSetupOptions.whisper.detail}</p>
                <span className="setup-option-meta">
                  Accepted: {modelSetupOptions.whisper.acceptedExtensions.map((extension) => `.${extension}`).join(", ")}
                </span>
                <span className="setup-option-meta">
                  {modelSetupOptions.whisper.downloadsManaged
                    ? "Managed downloads enabled"
                    : "Managed downloads unavailable"}
                </span>
              </div>
              <div className="setup-option-group">
                <strong>{modelSetupOptions.ollama.title}</strong>
                <p>{modelSetupOptions.ollama.detail}</p>
                <span className="setup-option-meta">
                  {modelSetupOptions.ollama.automaticPulls ? "Automatic pulls enabled" : "Manual pulls only"}
                </span>
                <div className="ollama-candidate-list">
                  {modelSetupOptions.ollama.candidates.map((candidate) => (
                    <div key={candidate.id} className="ollama-candidate-row">
                      <span>
                        <strong>{candidate.displayName}</strong>
                        <small>{candidate.modelTag}</small>
                      </span>
                      <span className="pull-command-copy">
                        <span className="setup-option-meta">{candidate.pullCommand}</span>
                        <CopyPullCommandButton
                          pullCommand={candidate.pullCommand}
                          disabled={commandBusy}
                          onCopy={copyOllamaPullCommand}
                        />
                      </span>
                      <button
                        type="button"
                        className="button"
                        disabled={settingsInputDisabled || settingsForm.ollamaModel === candidate.modelTag}
                        title="Use this model tag in the local settings form."
                        onClick={() => chooseOllamaCandidate(candidate.modelTag)}
                      >
                        Use
                      </button>
                    </div>
                  ))}
                </div>
              </div>
            </div>
            <div className="calendar-context" aria-label="Calendar context">
              <div className={`readiness-item ${calendarTone}`}>
                <div className="readiness-heading">
                  <StatusPill tone={calendarTone} label={calendarContextLabel(calendarContext)} />
                </div>
                <p>{calendarContext.message}</p>
                <p>{calendarContext.setupGuidance}</p>
                {calendarContext.upcomingEvents.length > 0 ? (
                  <div className="calendar-event-list">
                    {calendarContext.upcomingEvents.map((event) => (
                      <div key={event.id} className="calendar-event-row">
                        <strong>{event.title}</strong>
                        <span>{formatCalendarEventMetadata(event)}</span>
                        <small>{event.safetyNote}</small>
                        {event.attachable ? (
                          <button
                            type="button"
                            className="button"
                            disabled={!commandSurfaceReady || !selectedMeeting || commandBusy}
                            title={
                              !selectedMeeting
                                ? "Select a meeting before attaching calendar context."
                                : event.privacy === "Unknown"
                                  ? "Confirm this unknown-privacy event is safe to store as meeting context."
                                  : "Attach this event as meeting context."
                            }
                            onClick={() => attachCalendarEvent(event)}
                          >
                            <CalendarPlus size={16} weight="regular" />
                            {pendingCommand === "attach-calendar"
                              ? "Attaching"
                              : event.privacy === "Unknown"
                                ? "Confirm privacy and attach"
                                : "Attach to meeting"}
                          </button>
                        ) : null}
                      </div>
                    ))}
                  </div>
                ) : (
                  <small>No upcoming calendar events loaded.</small>
                )}
                <small>Auto-start disabled.</small>
                {calendarContext.permissionState === "NotRequested" ? (
                  <button
                    type="button"
                    className="button"
                    disabled={requestCalendarDisabled}
                    title={
                      commandSurfaceReady
                        ? "Request macOS Apple Calendar access for future manual event context."
                        : commandUnavailableTitle
                    }
                    onClick={requestCalendarAccess}
                  >
                    {pendingCommand === "request-calendar" ? "Requesting calendar" : "Request calendar access"}
                  </button>
                ) : null}
              </div>
            </div>
            <div className="settings-form" aria-label="Local settings">
              <div className="path-picker-control">
                <label className="settings-field" htmlFor="whisper-model-path">
                  <span>Whisper model path</span>
                  <input
                    id="whisper-model-path"
                    value={settingsForm.whisperModelPath}
                    onChange={(event) =>
                      setSettingsForm((current) => ({ ...current, whisperModelPath: event.target.value }))
                    }
                    placeholder="/absolute/path/to/ggml-base.en.bin"
                    disabled={settingsInputDisabled}
                  />
                </label>
                <button
                  type="button"
                  className="button"
                  disabled={chooseWhisperModelDisabled}
                  title={chooseWhisperModelButtonTitle}
                  onClick={chooseWhisperModelFile}
                >
                  <FolderOpen size={16} weight="regular" />
                  {pendingCommand === "choose-whisper-model" ? "Choosing model" : "Choose model"}
                </button>
              </div>
              <div className="settings-buttons">
                <button
                  type="button"
                  className="button"
                  disabled={settingsActionDisabled}
                  title={commandSurfaceReady ? "Test the configured Whisper path." : commandUnavailableTitle}
                  onClick={testWhisperModelPath}
                >
                  {pendingCommand === "test-whisper" ? "Testing path" : "Test path"}
                </button>
                <button
                  type="button"
                  className="button"
                  disabled={settingsActionDisabled}
                  title={commandSurfaceReady ? "Save the configured Whisper path." : commandUnavailableTitle}
                  onClick={saveWhisperModelPath}
                >
                  {pendingCommand === "save-whisper" ? "Saving Whisper" : "Save Whisper"}
                </button>
              </div>
              <label className="settings-field" htmlFor="ollama-base-url">
                <span>Ollama base URL</span>
                <input
                  id="ollama-base-url"
                  value={settingsForm.ollamaBaseUrl}
                  onChange={(event) => updateOllamaBaseUrl(event.target.value)}
                  placeholder="http://127.0.0.1:11434"
                  disabled={settingsInputDisabled}
                />
              </label>
              <label className="settings-field" htmlFor="ollama-model">
                <span>Ollama model</span>
                <input
                  id="ollama-model"
                  value={settingsForm.ollamaModel}
                  onChange={(event) => updateOllamaModel(event.target.value)}
                  placeholder="qwen3.6:27b"
                  disabled={settingsInputDisabled}
                />
              </label>
              <div className="settings-buttons">
                <button
                  type="button"
                  className="button"
                  disabled={settingsActionDisabled}
                  title={commandSurfaceReady ? "Test the configured local Ollama server and model." : commandUnavailableTitle}
                  onClick={testOllamaConnection}
                >
                  {pendingCommand === "test-ollama" ? "Testing Ollama" : "Test Ollama"}
                </button>
                <button
                  type="button"
                  className="button"
                  disabled={settingsActionDisabled}
                  title={commandSurfaceReady ? "Save local analysis settings." : commandUnavailableTitle}
                  onClick={saveAnalysisSettings}
                >
                  {pendingCommand === "save-analysis" ? "Saving analysis" : "Save analysis"}
                </button>
              </div>
              <label className="settings-field" htmlFor="raw-audio-retention">
                <span>Raw audio retention</span>
                <select
                  id="raw-audio-retention"
                  value={settingsForm.rawAudioRetentionPolicy}
                  onChange={(event) =>
                    updateRawAudioRetentionPolicy(event.target.value as PersistedRawAudioRetentionPolicy)
                  }
                  disabled={settingsInputDisabled}
                >
                  <option value="Retain">Retain</option>
                  <option value="DeleteAfterTranscription">Delete after transcription</option>
                </select>
              </label>
              <div className="settings-buttons">
                <button
                  type="button"
                  className="button"
                  disabled={settingsActionDisabled}
                  title={commandSurfaceReady ? "Save default raw-audio retention." : commandUnavailableTitle}
                  onClick={saveRawAudioRetentionPolicy}
                >
                  {pendingCommand === "save-retention" ? "Saving retention" : "Save retention"}
                </button>
              </div>
              {settingsFeedback ? (
                <div className={`settings-feedback ${settingsFeedback.tone}`} role="status">
                  <span>{settingsFeedback.message}</span>
                  {settingsFeedback.metadata ? (
                    <span className="settings-feedback-metadata">
                      {settingsFeedback.metadata.kind === "whisper" ? (
                        <>
                          <span>Size: {settingsFeedback.metadata.fileSizeBytes} bytes</span>
                          <span>SHA-256: {settingsFeedback.metadata.sha256}</span>
                        </>
                      ) : (
                        <>
                          {settingsFeedback.metadata.selectedLocalModelTag ? (
                            <span>Selected model: {settingsFeedback.metadata.selectedLocalModelTag}</span>
                          ) : null}
                          {settingsFeedback.metadata.installedLocalModels ? (
                            <span>
                              Installed models:{" "}
                              {settingsFeedback.metadata.installedLocalModels.length > 0
                                ? settingsFeedback.metadata.installedLocalModels.join(", ")
                                : "none reported"}
                            </span>
                          ) : null}
                          {settingsFeedback.metadata.pullCommand ? (
                            <span className="pull-command-copy">
                              <span>Pull command: {settingsFeedback.metadata.pullCommand}</span>
                              <CopyPullCommandButton
                                pullCommand={settingsFeedback.metadata.pullCommand}
                                disabled={commandBusy}
                                onCopy={copyOllamaPullCommand}
                              />
                            </span>
                          ) : null}
                        </>
                      )}
                    </span>
                  ) : null}
                </div>
              ) : null}
            </div>
          </aside>
        </div>
      </section>
    </main>
  );
}

function StatusPill({ tone, label }: { tone: Tone; label: string }) {
  return <span className={`status-pill ${tone}`}>{label}</span>;
}

function CopyPullCommandButton({
  pullCommand,
  disabled,
  onCopy,
}: {
  pullCommand: string;
  disabled: boolean;
  onCopy(pullCommand: string): Promise<void>;
}) {
  const modelLabel = ollamaPullCommandModelLabel(pullCommand);
  return (
    <button
      type="button"
      className="button quiet pull-command-copy-button"
      disabled={disabled}
      title="Copy this pull command to the clipboard."
      aria-label={`Copy pull command for ${modelLabel}`}
      onClick={() => {
        void onCopy(pullCommand);
      }}
    >
      <CopySimple size={14} weight="regular" />
      Copy
    </button>
  );
}

function ollamaPullCommandModelLabel(pullCommand: string) {
  const parts = pullCommand.trim().split(/\s+/);
  if (parts[0] === "ollama" && parts[1] === "pull" && parts.length > 2) {
    return parts.slice(2).join(" ");
  }
  return pullCommand;
}

function whisperSetupLabel(state: DesktopSnapshot["setupGuidance"]["whisper"]["state"]) {
  if (state === "ReadablePath") {
    return "Whisper path readable";
  }
  if (state === "UnreadablePath") {
    return "Whisper path blocked";
  }
  if (state === "UnsupportedFile") {
    return "Whisper file unsupported";
  }
  return "Whisper path missing";
}

function whisperSetupTone(state: DesktopSnapshot["setupGuidance"]["whisper"]["state"]): Tone {
  if (state === "ReadablePath") {
    return "warn";
  }
  return "blocked";
}

function ollamaSetupLabel(guidance: DesktopSnapshot["setupGuidance"]["ollama"]) {
  if (guidance.state === "InvalidLocalConfiguration") {
    return "Ollama setup invalid";
  }
  if (guidance.availability === "AvailableAtLastTest") {
    return "Ollama available at last test";
  }
  if (guidance.availability === "MissingModelAtLastTest") {
    return "Ollama model missing";
  }
  if (guidance.availability === "UnavailableAtLastTest") {
    return "Summaries unavailable";
  }
  return "Ollama availability unknown";
}

function ollamaSetupTone(guidance: DesktopSnapshot["setupGuidance"]["ollama"]): Tone {
  if (
    guidance.state === "InvalidLocalConfiguration" ||
    guidance.availability === "MissingModelAtLastTest" ||
    guidance.availability === "UnavailableAtLastTest"
  ) {
    return "blocked";
  }
  if (guidance.availability === "AvailableAtLastTest") {
    return "ready";
  }
  return "warn";
}

function calendarContextLabel(context: DesktopSnapshot["calendarContext"]) {
  if (context.permissionState === "Denied") {
    return "Calendar access denied";
  }
  if (context.availabilityState === "Ready") {
    return "Calendar context ready";
  }
  if (context.permissionState === "NotRequested") {
    return "Calendar permission needed";
  }
  return "Calendar unavailable";
}

function calendarContextTone(context: DesktopSnapshot["calendarContext"]): Tone {
  if (context.permissionState === "Denied") {
    return "blocked";
  }
  if (context.availabilityState === "Ready") {
    return "ready";
  }
  if (context.availabilityState === "PermissionRequired" || context.permissionState === "NotRequested") {
    return "warn";
  }
  return "muted";
}

function StatusLine({
  icon,
  label,
  value,
  tone,
}: {
  icon: React.ReactNode;
  label: string;
  value: string;
  tone: Tone;
}) {
  return (
    <div className={`status-line ${tone}`}>
      <span className="status-icon">{icon}</span>
      <span>
        <strong>{label}</strong>
        <small>{value}</small>
      </span>
    </div>
  );
}

function IconFrame({ children, tone }: { children: React.ReactNode; tone: Tone }) {
  return <span className={`icon-frame ${tone}`}>{children}</span>;
}

function SkeletonList() {
  return (
    <div className="skeleton-list" aria-label="Loading workspace">
      <p>Loading workspace</p>
      <span />
      <span />
      <span />
    </div>
  );
}

function settingsFormFromSnapshot(snapshot: DesktopSnapshot): SettingsFormState {
  return {
    whisperModelPath: snapshot.settings.whisperModelPath,
    ollamaBaseUrl: snapshot.settings.ollamaBaseUrl,
    ollamaModel: snapshot.settings.ollamaModel,
    rawAudioRetentionPolicy: snapshot.settings.rawAudioRetentionPolicy,
  };
}

function selectedTitleFromSnapshot(snapshot: DesktopSnapshot): string {
  const selected = snapshot.meetings.find((meeting) => meeting.id === snapshot.selectedMeetingId);
  return selected?.title ?? snapshot.meetings[0]?.title ?? "";
}

function resolveSelectedMeetingId(snapshot: DesktopSnapshot, current: string | null): string | null {
  const backendSelected = snapshot.selectedMeetingId;
  if (backendSelected && snapshot.meetings.some((meeting) => meeting.id === backendSelected)) {
    return backendSelected;
  }
  if (current && snapshot.meetings.some((meeting) => meeting.id === current)) {
    return current;
  }
  return snapshot.meetings[0]?.id ?? null;
}

function commandAllowedDuringBusy(
  next: Exclude<PendingCommand, null>,
  current: PendingCommand,
): boolean {
  return (
    (current === "transcribe" && next === "cancel-transcription") ||
    (current === "summary" && next === "cancel-summary")
  );
}

function snapshotHasActiveCommandJob(snapshot: DesktopSnapshot): boolean {
  return (
    snapshot.transcriptionJob?.state === "Running" ||
    snapshot.transcriptionJob?.state === "CancelRequested" ||
    snapshot.summaryJob?.state === "Running" ||
    snapshot.summaryJob?.state === "CancelRequested"
  );
}

function isActiveCommandJob(job: DesktopSnapshot["transcriptionJob"]): boolean {
  return job?.state === "Running" || job?.state === "CancelRequested";
}

function isSelectedActiveCommandJob(
  job: DesktopSnapshot["transcriptionJob"],
  selectedMeetingId: string | undefined,
): boolean {
  return Boolean(job && selectedMeetingId && job.meetingId === selectedMeetingId && isActiveCommandJob(job));
}

function isSelectedRetryableJob(
  job: DesktopSnapshot["transcriptionJob"],
  selectedMeetingId: string | undefined,
): boolean {
  return Boolean(
    job &&
      selectedMeetingId &&
      job.meetingId === selectedMeetingId &&
      (job.state === "Failed" || job.state === "Recovery" || job.state === "Retry"),
  );
}

function preserveCommandJobProgress(
  current: DesktopSnapshot,
  next: DesktopSnapshot,
): DesktopSnapshot {
  return {
    ...next,
    transcriptionJob: preserveJobProgress(current.transcriptionJob, next.transcriptionJob),
    summaryJob: preserveJobProgress(current.summaryJob, next.summaryJob),
  };
}

function preserveJobProgress<T extends DesktopSnapshot["transcriptionJob"]>(
  current: T,
  next: T,
): T {
  if (!current || !next || current.id !== next.id) {
    return next;
  }
  if (current.state !== "Running" && next.state === "Running") {
    return current;
  }
  return next;
}

function captureLabel(state: DesktopSnapshot["capture"]["microphone"]) {
  return mapPermissionState(state).label;
}

function captureDetail(state: DesktopSnapshot["capture"]["microphone"]) {
  return mapPermissionState(state).detail;
}

function captureTone(state: DesktopSnapshot["capture"]["microphone"]): Tone {
  return mapPermissionState(state).tone;
}

function commandErrorMessage(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }
  if (typeof error === "string") {
    return error;
  }
  return "Desktop command failed.";
}

function formatTime(ms: number) {
  const seconds = Math.floor(ms / 1000);
  const minutes = Math.floor(seconds / 60);
  return `${minutes}:${String(seconds % 60).padStart(2, "0")}`;
}

function formatEvidenceTimestamp(ms: number) {
  return new Date(ms).toISOString();
}

function formatCalendarEventMetadata(event: DesktopSnapshot["calendarContext"]["upcomingEvents"][number]) {
  const flags = [
    formatCalendarEventRange(event.startsAtMs, event.endsAtMs),
    event.calendarTitle,
    `${event.privacy} privacy`,
    event.overlapState !== "None" ? event.overlapState : null,
    event.isAllDay ? "All day" : null,
    event.isRecurring ? "Recurring" : null,
  ].filter(Boolean);

  return flags.join(" / ");
}

function formatMeetingCalendarAttachment(
  attachment: NonNullable<DesktopSnapshot["meetings"][number]["calendarAttachment"]>,
) {
  const privacy =
    attachment.privacy === "Unknown" && attachment.privacyConfirmed
      ? "Unknown privacy confirmed"
      : `${attachment.privacy} privacy`;
  return [
    attachment.eventTitle,
    formatCalendarEventRange(attachment.startsAtMs, attachment.endsAtMs),
    attachment.calendarTitle,
    privacy,
  ].join(" / ");
}

function formatCalendarEventRange(startsAtMs: number, endsAtMs: number) {
  const start = new Date(startsAtMs);
  const end = new Date(endsAtMs);
  return `${start.toLocaleString([], {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  })}-${end.toLocaleTimeString([], {
    hour: "2-digit",
    minute: "2-digit",
  })}`;
}
