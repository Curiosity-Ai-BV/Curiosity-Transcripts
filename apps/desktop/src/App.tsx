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
  mapRawAudioRetention,
  mapRecordingState,
  mapTranscriptionState,
  searchMeetings,
  Tone,
} from "./commandAdapter";
import { RecordingControls } from "./desktopRecordingControls";
import { MeetingPane } from "./desktopMeetingPane";
import { MeetingDetailHeader } from "./desktopMeetingDetailHeader";
import { MeetingPrivacyRow } from "./desktopMeetingPrivacyRow";
import { MeetingSummarySection } from "./desktopMeetingSummarySection";
import { MeetingDetailActions } from "./desktopMeetingDetailActions";
import { MeetingTranscriptSection } from "./desktopMeetingTranscriptSection";
import { DesktopCommandOutcomes } from "./desktopCommandOutcomes";
import { DesktopCalendarContext } from "./desktopCalendarContext";
import { DesktopModelReadiness } from "./desktopModelReadiness";
import { DesktopModelSetupOptions } from "./desktopModelSetupOptions";
import { DesktopSettingsEngineStack } from "./desktopSettingsEngineStack";
import { DesktopSettingsForm } from "./desktopSettingsForm";
import type { SettingsFeedback } from "./desktopSettingsFeedback";
import { DesktopTopbar } from "./desktopTopbar";
import {
  ACTIVE_JOB_POLL_INTERVAL_MS,
  calendarContextLabel,
  calendarContextTone,
  captureDetail,
  captureLabel,
  captureTone,
  commandAllowedDuringBusy,
  commandErrorMessage,
  formatMeetingCalendarAttachment,
  isActiveCommandJob,
  isSelectedActiveCommandJob,
  isSelectedRetryableJob,
  ollamaSetupLabel,
  ollamaSetupTone,
  ollamaSummaryBlocked,
  preserveCommandJobProgress,
  resolveSelectedMeetingId,
  selectedTitleFromSnapshot,
  settingsFormFromSnapshot,
  snapshotHasActiveCommandJob,
  whisperSetupLabel,
  whisperSetupTone,
} from "./desktopWorkspaceState";
import type { PendingCommand, SettingsFormState } from "./desktopWorkspaceState";

import "./styles.css";

const appVersion = packageInfo.version;

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
  const ollamaSummaryBlockGuidance = ollamaSummaryBlocked(setupGuidance.ollama)
    ? setupGuidance.ollama.setupGuidance
    : null;
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
  const segmentDraftDisabled = pendingCommand === "correct-segment";
  const saveCorrectionTitle = commandSurfaceReady
    ? "Save the user correction for this transcript segment."
    : commandUnavailableTitle;
  const cancelCorrectionDisabled = pendingCommand === "correct-segment";
  const editSegmentDisabled = !commandSurfaceReady || commandBusy;
  const editSegmentTitle = commandSurfaceReady ? "Edit this transcript segment." : commandUnavailableTitle;
  const settingsInputDisabled = commandBusy;
  const settingsActionDisabled = commandBusy;
  const chooseWhisperModelDisabled = !commandSurfaceReady || commandBusy;
  const requestCalendarDisabled =
    !commandSurfaceReady || commandBusy || calendarContext.permissionState !== "NotRequested";
  const requestCalendarTitle = commandSurfaceReady
    ? "Request macOS Apple Calendar access for future manual event context."
    : commandUnavailableTitle;
  const hasSelectedMeeting = Boolean(selectedMeeting);
  const canAttachCalendarEvents = commandSurfaceReady && hasSelectedMeeting && !commandBusy;

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
  const selectedMeetingCalendarContext = selectedMeeting?.calendarAttachment
    ? formatMeetingCalendarAttachment(selectedMeeting.calendarAttachment)
    : null;
  const exportCommandState = mapExportState(currentSnapshot.exportCommand);
  const deleteCommandState = mapDeleteState(currentSnapshot.deleteCommand);
  const failedDeleteMeetingId =
    currentSnapshot.deleteCommand.state === "failed" ? currentSnapshot.deleteCommand.meetingId?.trim() : undefined;
  const retryDeleteDisabled = !commandSurfaceReady || commandBusy;
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
    selectedMeeting.segments.length === 0 ||
    Boolean(ollamaSummaryBlockGuidance);
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
    Boolean(ollamaSummaryBlockGuidance) ||
    commandBusy;
  const microphoneStatus = {
    label: captureLabel(currentSnapshot.capture.microphone),
    value: captureDetail(currentSnapshot.capture.microphone),
    tone: captureTone(currentSnapshot.capture.microphone),
  };
  const systemAudioStatus = {
    label: captureLabel(currentSnapshot.capture.systemAudio),
    value: captureDetail(currentSnapshot.capture.systemAudio),
    tone: captureTone(currentSnapshot.capture.systemAudio),
  };
  const calendarStatus = {
    label: calendarContextLabel(calendarContext),
    value: calendarContext.message,
    tone: calendarTone,
  };
  const selectedMeetingAnalysisStatus = selectedMeeting
    ? {
        label: analysisDisclosure?.label ?? "Summary unavailable",
        value: analysisDisclosure?.detail ?? "No selected meeting.",
        tone: analysisDisclosure?.tone ?? ("muted" as Tone),
      }
    : null;
  const cancelTranscriptionButtonTitle = commandSurfaceReady
    ? "Request cancellation for the active transcription job."
    : commandUnavailableTitle;
  const cancelSummaryButtonTitle = commandSurfaceReady
    ? "Request cancellation for the active summary job."
    : commandUnavailableTitle;

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
      const testedPathIsActive =
        result.state === "Valid" &&
        testedPathTrimmed !== "" &&
        (testedPathTrimmed === savedWhisperPath || testedPathTrimmed === effectiveWhisperPath);
      if (testedPathIsActive) {
        const nextSnapshot = await commandFacade.desktopSnapshot();
        applyDesktopSnapshot(nextSnapshot);
      }
      const testedPathNeedsSave = result.state === "Valid" && testedPathTrimmed !== "" && !testedPathIsActive;
      const validPathMessage = result.message || "Whisper model path is readable.";
      setSettingsFeedback({
        tone: result.state === "Valid" ? "ready" : "blocked",
        message: testedPathNeedsSave
          ? `${validPathMessage} Save Whisper to make this path active.`
          : result.message || result.setupGuidance,
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

  function updateRawAudioRetentionPolicy(value: SettingsFormState["rawAudioRetentionPolicy"]) {
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
  const retryDeleteTitle = commandBusy ? busyCommandTitle : "Retry deletion for the failed meeting.";
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
  const testWhisperButtonTitle = commandSurfaceReady ? "Test the configured Whisper path." : commandUnavailableTitle;
  const saveWhisperButtonTitle = commandSurfaceReady ? "Save the configured Whisper path." : commandUnavailableTitle;
  const testOllamaButtonTitle = commandSurfaceReady
    ? "Test the configured local Ollama server and model."
    : commandUnavailableTitle;
  const saveAnalysisButtonTitle = commandSurfaceReady ? "Save local analysis settings." : commandUnavailableTitle;
  const saveRetentionButtonTitle = commandSurfaceReady
    ? "Save default raw-audio retention."
    : commandUnavailableTitle;
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
          : ollamaSummaryBlockGuidance
            ? ollamaSummaryBlockGuidance
          : "Generate a local Ollama summary for the selected meeting.";
  const isLightTheme = theme === "light";
  const themeButtonLabel = isLightTheme ? "Switch to dark mode" : "Switch to light mode";

  function toggleTheme() {
    setTheme((current) => (current === "dark" ? "light" : "dark"));
  }

  const recordingControls = (
    <RecordingControls
      recording={recording}
      recordingTitle={recordingTitle}
      importWavPath={importWavPath}
      recordingTitleDisabled={recordingTitleDisabled}
      importWavPathDisabled={importWavPathDisabled}
      chooseWavDisabled={chooseWavDisabled}
      startDisabled={startDisabled}
      importDisabled={importDisabled}
      stopDisabled={stopDisabled}
      chooseWavButtonTitle={chooseWavButtonTitle}
      startButtonTitle={startButtonTitle}
      importButtonTitle={importButtonTitle}
      stopButtonTitle={stopButtonTitle}
      storagePath={currentSnapshot.recording.storage_location.app_private_path}
      pendingCommand={pendingCommand}
      onRecordingTitleChange={setRecordingTitle}
      onImportWavPathChange={setImportWavPath}
      onChooseWav={chooseImportWavFile}
      onStartRecording={startRecording}
      onImportWav={importWavFile}
      onStopRecording={stopRecording}
    />
  );

  return (
    <main className="app-shell" data-theme={theme}>
      <section className="workspace" aria-label="Transcript workspace">
        <DesktopTopbar
          appVersion={appVersion}
          isLightTheme={isLightTheme}
          themeButtonLabel={themeButtonLabel}
          onToggleTheme={toggleTheme}
        />

        <DesktopCommandOutcomes
          commandError={commandError}
          showExportOutcome={currentSnapshot.exportCommand.state !== "idle"}
          exportCommandState={exportCommandState}
          showDeleteOutcome={currentSnapshot.deleteCommand.state !== "idle"}
          deleteCommandState={deleteCommandState}
          failedDeleteMeetingId={failedDeleteMeetingId}
          retryDeleteDisabled={retryDeleteDisabled}
          retryDeleteTitle={retryDeleteTitle}
          summaryFailure={summaryFailure ?? null}
          onRetryDelete={retryFailedDelete}
        />

        <div className="content-grid">
          <MeetingPane
            query={query}
            meetings={meetings}
            selectedMeetingId={selectedMeeting?.id ?? null}
            loading={currentSnapshot.loading}
            onQueryChange={setQuery}
            onSelectMeeting={setSelectedMeetingId}
          />

          <section className="detail-pane" aria-label="Meeting detail">
            {selectedMeeting ? (
              <>
                {recordingControls}

                <MeetingDetailHeader
                  meeting={selectedMeeting}
                  renameTitle={renameTitle}
                  renameInputDisabled={!commandSurfaceReady || commandBusy}
                  renameDisabled={renameDisabled}
                  transcribeDisabled={transcribeDisabled}
                  renameButtonTitle={renameButtonTitle}
                  transcribeButtonTitle={transcribeButtonTitle}
                  pendingCommand={pendingCommand}
                  onRenameTitleChange={setRenameTitle}
                  onRename={renameSelectedMeeting}
                  onTranscribe={transcribeSelectedMeeting}
                />

                <MeetingPrivacyRow
                  storage={{
                    label: selectedMeeting.privacy.storageLabel,
                    path: selectedMeeting.privacy.storagePath,
                  }}
                  rawAudioRetention={rawAudioRetention}
                  localProcessing={localProcessingState}
                  calendarContext={selectedMeetingCalendarContext}
                  exportState={exportState}
                  deleteState={deleteState}
                />

                {analysisDisclosure ? (
                  <MeetingSummarySection
                    disclosure={analysisDisclosure}
                    summaryText={selectedMeeting.analysis?.summary ?? null}
                    summaryDisabled={summaryDisabled}
                    summaryButtonTitle={summaryButtonTitle}
                    pendingCommand={pendingCommand}
                    onGenerateSummary={generateSelectedSummary}
                  />
                ) : null}

                <MeetingTranscriptSection
                  segments={selectedMeeting.segments}
                  editingSegmentId={editingSegmentId}
                  segmentDraft={segmentDraft}
                  segmentDraftDisabled={segmentDraftDisabled}
                  correctionDisabled={correctionDisabled}
                  saveCorrectionTitle={saveCorrectionTitle}
                  cancelCorrectionDisabled={cancelCorrectionDisabled}
                  editSegmentDisabled={editSegmentDisabled}
                  editSegmentTitle={editSegmentTitle}
                  pendingCommand={pendingCommand}
                  onSegmentDraftChange={setSegmentDraft}
                  onEditSegment={editTranscriptSegment}
                  onCancelCorrection={cancelTranscriptCorrection}
                  onSaveCorrection={saveTranscriptCorrection}
                />

                <MeetingDetailActions
                  selectedExportFormat={selectedExportFormat}
                  selectedExportFormatLabel={selectedExportFormatLabel}
                  exportDisabled={exportDisabled}
                  deleteDisabled={deleteDisabled}
                  exportButtonTitle={exportButtonTitle}
                  deleteButtonTitle={deleteButtonTitle}
                  pendingCommand={pendingCommand}
                  onExportFormatChange={setSelectedExportFormat}
                  onExport={exportSelectedMeeting}
                  onDelete={deleteSelectedMeeting}
                />
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
            <DesktopSettingsEngineStack
              model={{ label: model.label, value: model.detail, tone: model.tone }}
              transcription={{
                label: transcription.label,
                value: transcription.detail,
                tone: transcription.tone,
              }}
              transcriptionJob={
                transcriptionJob
                  ? {
                      label: transcriptionJob.label,
                      value: transcriptionJob.detail,
                      tone: transcriptionJob.tone,
                    }
                  : null
              }
              summaryJob={
                summaryJob
                  ? {
                      label: summaryJob.label,
                      value: summaryJob.detail,
                      tone: summaryJob.tone,
                    }
                  : null
              }
              microphone={microphoneStatus}
              systemAudio={systemAudioStatus}
              calendar={calendarStatus}
              selectedMeetingAnalysis={selectedMeetingAnalysisStatus}
              canCancelTranscriptionJob={canCancelTranscriptionJob}
              canRetryTranscriptionJob={canRetryTranscriptionJob}
              canCancelSummaryJob={canCancelSummaryJob}
              canRetrySummaryJob={canRetrySummaryJob}
              cancelTranscriptionDisabled={cancelTranscriptionDisabled}
              retryTranscriptionDisabled={retryTranscriptionDisabled}
              cancelSummaryDisabled={cancelSummaryDisabled}
              retrySummaryDisabled={retrySummaryDisabled}
              cancelTranscriptionButtonTitle={cancelTranscriptionButtonTitle}
              retryTranscriptionButtonTitle={retryTranscriptionButtonTitle}
              cancelSummaryButtonTitle={cancelSummaryButtonTitle}
              summaryButtonTitle={summaryButtonTitle}
              pendingCommand={pendingCommand}
              onCancelTranscriptionJob={cancelTranscriptionJob}
              onRetryTranscriptionJob={transcribeSelectedMeeting}
              onCancelSummaryJob={cancelSummaryJob}
              onRetrySummaryJob={generateSelectedSummary}
            />
            <DesktopModelReadiness
              whisper={setupGuidance.whisper}
              whisperLabel={whisperSetupLabel(setupGuidance.whisper.state)}
              whisperTone={whisperReadinessTone}
              ollama={setupGuidance.ollama}
              ollamaLabel={ollamaSetupLabel(setupGuidance.ollama)}
              ollamaTone={ollamaReadinessTone}
              copyPullCommandDisabled={commandBusy}
              onCopyPullCommand={copyOllamaPullCommand}
            />
            <DesktopModelSetupOptions
              options={modelSetupOptions}
              selectedOllamaModel={settingsForm.ollamaModel}
              settingsInputDisabled={settingsInputDisabled}
              copyPullCommandDisabled={commandBusy}
              onCopyPullCommand={copyOllamaPullCommand}
              onChooseOllamaCandidate={chooseOllamaCandidate}
            />
            <DesktopCalendarContext
              context={calendarContext}
              label={calendarContextLabel(calendarContext)}
              tone={calendarTone}
              pendingCommand={pendingCommand}
              requestCalendarDisabled={requestCalendarDisabled}
              requestCalendarTitle={requestCalendarTitle}
              canAttachEvents={canAttachCalendarEvents}
              hasSelectedMeeting={hasSelectedMeeting}
              onRequestCalendarAccess={requestCalendarAccess}
              onAttachCalendarEvent={attachCalendarEvent}
            />
            <DesktopSettingsForm
              settingsForm={settingsForm}
              settingsFeedback={settingsFeedback}
              pendingCommand={pendingCommand}
              settingsInputDisabled={settingsInputDisabled}
              settingsActionDisabled={settingsActionDisabled}
              chooseWhisperModelDisabled={chooseWhisperModelDisabled}
              chooseWhisperModelButtonTitle={chooseWhisperModelButtonTitle}
              testWhisperButtonTitle={testWhisperButtonTitle}
              saveWhisperButtonTitle={saveWhisperButtonTitle}
              testOllamaButtonTitle={testOllamaButtonTitle}
              saveAnalysisButtonTitle={saveAnalysisButtonTitle}
              saveRetentionButtonTitle={saveRetentionButtonTitle}
              copyPullCommandDisabled={commandBusy}
              onWhisperModelPathChange={(value) =>
                setSettingsForm((current) => ({ ...current, whisperModelPath: value }))
              }
              onChooseWhisperModel={chooseWhisperModelFile}
              onTestWhisperModelPath={testWhisperModelPath}
              onSaveWhisperModelPath={saveWhisperModelPath}
              onOllamaBaseUrlChange={updateOllamaBaseUrl}
              onOllamaModelChange={updateOllamaModel}
              onTestOllamaConnection={testOllamaConnection}
              onSaveAnalysisSettings={saveAnalysisSettings}
              onRawAudioRetentionPolicyChange={updateRawAudioRetentionPolicy}
              onSaveRawAudioRetentionPolicy={saveRawAudioRetentionPolicy}
              onCopyPullCommand={copyOllamaPullCommand}
            />
          </aside>
        </div>
      </section>
    </main>
  );
}
