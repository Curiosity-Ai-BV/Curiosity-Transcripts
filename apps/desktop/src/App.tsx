import {
  CheckCircle,
  DownloadSimple,
  FileText,
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
import { useEffect, useMemo, useState } from "react";

import packageInfo from "../package.json";
import {
  CommandFetcher,
  DesktopSnapshot,
  getMockDesktopSnapshot,
  mapAnalysisDisclosure,
  mapDeleteState,
  mapExportState,
  mapModelStatus,
  mapPermissionState,
  mapRecordingState,
  mapTranscriptionState,
  MeetingSearchResult,
  OllamaConnectionTestResult,
  searchMeetings,
  Tone,
  WhisperModelPathTestResult,
} from "./commandAdapter";

import "./styles.css";

const appVersion = packageInfo.version;

interface AppProps {
  snapshot?: DesktopSnapshot;
  fetchCommand?: CommandFetcher;
}

type PendingCommand =
  | "start"
  | "stop"
  | "transcribe"
  | "rename"
  | "export"
  | "delete"
  | "summary"
  | "test-whisper"
  | "test-ollama"
  | "save-whisper"
  | "save-analysis"
  | null;

type ThemeMode = "dark" | "light";

interface SettingsFormState {
  whisperModelPath: string;
  ollamaBaseUrl: string;
  ollamaModel: string;
}

interface SettingsFeedback {
  tone: Tone;
  message: string;
}

const connectedCommandSurface = "Connected to local desktop commands.";

export default function App({ snapshot, fetchCommand }: AppProps) {
  const initialSnapshot = snapshot ?? getMockDesktopSnapshot();
  const [currentSnapshot, setCurrentSnapshot] = useState(initialSnapshot);
  const [query, setQuery] = useState("");
  const [connectedSearchResultIds, setConnectedSearchResultIds] = useState<string[] | null>(null);
  const [selectedMeetingId, setSelectedMeetingId] = useState(initialSnapshot.selectedMeetingId);
  const [renameTitle, setRenameTitle] = useState(selectedTitleFromSnapshot(initialSnapshot));
  const [recordingTitle, setRecordingTitle] = useState("");
  const [settingsForm, setSettingsForm] = useState<SettingsFormState>(settingsFormFromSnapshot(initialSnapshot));
  const [settingsFeedback, setSettingsFeedback] = useState<SettingsFeedback | null>(null);
  const [pendingCommand, setPendingCommand] = useState<PendingCommand>(null);
  const [commandError, setCommandError] = useState<string | null>(null);
  const [theme, setTheme] = useState<ThemeMode>("dark");

  useEffect(() => {
    if (snapshot) {
      setCurrentSnapshot(snapshot);
      setSettingsForm(settingsFormFromSnapshot(snapshot));
      setConnectedSearchResultIds(null);
      setRenameTitle(selectedTitleFromSnapshot(snapshot));
      setSettingsFeedback(null);
      setCommandError(null);
      setPendingCommand(null);
    }
  }, [snapshot]);

  const commandUnavailable = currentSnapshot.commandSurface.detail;
  const commandSurfaceReady = Boolean(fetchCommand && commandUnavailable === connectedCommandSurface);
  const commandUnavailableTitle = commandSurfaceReady
    ? ""
    : fetchCommand || commandUnavailable.startsWith("Preview shell")
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
  }, [selectedMeeting?.id, selectedMeeting?.title]);

  useEffect(() => {
    if (!fetchCommand || !commandSurfaceReady) {
      setConnectedSearchResultIds(null);
      return;
    }
    const searchQuery = query.trim();
    if (!searchQuery) {
      setConnectedSearchResultIds(null);
      return;
    }

    let cancelled = false;
    fetchCommand<MeetingSearchResult[]>("search_meetings", { query: searchQuery })
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
  }, [commandSurfaceReady, fetchCommand, query]);

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
  const startDisabled = !commandSurfaceReady || isRecordingActive || commandBusy;
  const stopDisabled = !commandSurfaceReady || !isRecordingActive || commandBusy;
  const transcribeDisabled = !commandSurfaceReady || !selectedMeeting || commandBusy;
  const renameDisabled =
    !commandSurfaceReady ||
    !selectedMeeting ||
    commandBusy ||
    !renameTitle.trim() ||
    renameTitle.trim() === selectedMeeting.title;
  const exportDisabled = !commandSurfaceReady || !selectedMeeting || commandBusy;
  const deleteDisabled = !commandSurfaceReady || !selectedMeeting || commandBusy;
  const recordingTitleDisabled = !commandSurfaceReady || isRecordingActive || commandBusy;
  const settingsInputDisabled = commandBusy;
  const settingsActionDisabled = commandBusy;

  const exportState = selectedMeeting
    ? mapExportState(selectedMeeting.exportState)
    : mapExportState({ state: "idle" });
  const deleteState = selectedMeeting
    ? mapDeleteState(selectedMeeting.deleteState)
    : mapDeleteState({ state: "idle" });
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

  async function runSnapshotCommand(
    pending: Exclude<PendingCommand, null>,
    command: string,
    args?: Record<string, unknown>,
  ) {
    if (!fetchCommand || !commandSurfaceReady || commandBusy) {
      return;
    }

    setPendingCommand(pending);
    setCommandError(null);
    try {
      const nextSnapshot = await fetchCommand<DesktopSnapshot>(command, args);
      setCurrentSnapshot(nextSnapshot);
      setSelectedMeetingId((current) => resolveSelectedMeetingId(nextSnapshot, current));
      setRecordingTitle("");
    } catch (error) {
      setCommandError(commandErrorMessage(error));
    } finally {
      setPendingCommand(null);
    }
  }

  function startRecording() {
    const title = recordingTitle.trim();
    void runSnapshotCommand(
      "start",
      "start_microphone_recording",
      title ? { title } : undefined,
    );
  }

  function stopRecording() {
    void runSnapshotCommand("stop", "stop_microphone_recording");
  }

  function transcribeSelectedMeeting() {
    if (!selectedMeeting) {
      return;
    }
    void runSnapshotCommand("transcribe", "transcribe_meeting", {
      meetingId: selectedMeeting.id,
    });
  }

  function renameSelectedMeeting() {
    if (!selectedMeeting) {
      return;
    }
    const title = renameTitle.trim();
    if (!title) {
      return;
    }
    void runSnapshotCommand("rename", "rename_meeting", {
      meetingId: selectedMeeting.id,
      title,
    });
  }

  function exportSelectedMeeting() {
    if (!selectedMeeting) {
      return;
    }
    void runSnapshotCommand("export", "export_meeting_json", {
      meetingId: selectedMeeting.id,
    });
  }

  function deleteSelectedMeeting() {
    if (!selectedMeeting) {
      return;
    }
    void runSnapshotCommand("delete", "delete_meeting", {
      meetingId: selectedMeeting.id,
    });
  }

  function generateSelectedSummary() {
    if (!selectedMeeting) {
      return;
    }
    void runSnapshotCommand("summary", "generate_summary", {
      meetingId: selectedMeeting.id,
    });
  }

  function retryFailedDelete() {
    if (!failedDeleteMeetingId) {
      return;
    }
    void runSnapshotCommand("delete", "delete_meeting", {
      meetingId: failedDeleteMeetingId,
    });
  }

  async function runSettingsSnapshotCommand(
    pending: Exclude<PendingCommand, null>,
    command: string,
    args: Record<string, unknown>,
    successMessage: string,
  ) {
    if (commandBusy) {
      return;
    }
    if (!fetchCommand || !commandSurfaceReady) {
      setSettingsFeedback({ tone: "blocked", message: commandUnavailableTitle });
      return;
    }

    setPendingCommand(pending);
    setCommandError(null);
    setSettingsFeedback(null);
    try {
      const nextSnapshot = await fetchCommand<DesktopSnapshot>(command, args);
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
    if (!fetchCommand || !commandSurfaceReady) {
      setSettingsFeedback({ tone: "blocked", message: commandUnavailableTitle });
      return;
    }

    setPendingCommand("test-whisper");
    setCommandError(null);
    setSettingsFeedback(null);
    try {
      const result = await fetchCommand<WhisperModelPathTestResult>("test_whisper_model_path", {
        path: settingsForm.whisperModelPath,
      });
      setSettingsFeedback({
        tone: result.state === "Valid" ? "ready" : "blocked",
        message: result.message || result.setupGuidance,
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
    if (!fetchCommand || !commandSurfaceReady) {
      setSettingsFeedback({ tone: "blocked", message: commandUnavailableTitle });
      return;
    }

    setPendingCommand("test-ollama");
    setCommandError(null);
    setSettingsFeedback(null);
    try {
      const result = await fetchCommand<OllamaConnectionTestResult>("test_ollama_connection", {
        baseUrl: settingsForm.ollamaBaseUrl,
        model: settingsForm.ollamaModel,
      });
      setSettingsFeedback({
        tone: result.state === "Available" ? "ready" : "blocked",
        message: result.message || result.setupGuidance,
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

  function saveWhisperModelPath() {
    void runSettingsSnapshotCommand(
      "save-whisper",
      "save_whisper_model_path",
      { whisperModelPath: settingsForm.whisperModelPath },
      "Whisper model path saved.",
    );
  }

  function saveAnalysisSettings() {
    void runSettingsSnapshotCommand(
      "save-analysis",
      "save_analysis_settings",
      {
        ollamaBaseUrl: settingsForm.ollamaBaseUrl,
        ollamaModel: settingsForm.ollamaModel,
      },
      "Analysis settings saved.",
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
  const transcribeButtonTitle = !commandSurfaceReady
    ? commandUnavailableTitle
    : commandBusy
      ? busyCommandTitle
      : selectedMeeting
        ? "Transcribe the selected meeting with the configured local Whisper model."
        : "Select a meeting before transcription.";
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
        ? "Export the selected meeting as JSON."
        : "Select a meeting before exporting.";
  const deleteButtonTitle = !commandSurfaceReady
    ? commandUnavailableTitle
    : commandBusy
      ? busyCommandTitle
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

                <div className="privacy-row">
                  <StatusLine icon={<ShieldCheck size={18} weight="regular" />} label={selectedMeeting.privacy.storageLabel} value={selectedMeeting.privacy.storagePath} tone="ready" />
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
                    {selectedMeeting.segments.map((segment) => (
                      <article key={segment.id} className="segment">
                        <time>{formatTime(segment.startMs)}</time>
                        <p>{segment.text}</p>
                        <span>{segment.sourceChannel}</span>
                      </article>
                    ))}
                  </div>
                </section>

                <div className="detail-actions">
                  <button
                    type="button"
                    className="button"
                    disabled={exportDisabled}
                    title={exportButtonTitle}
                    onClick={exportSelectedMeeting}
                  >
                    <DownloadSimple size={16} weight="regular" />
                    {pendingCommand === "export" ? "Exporting JSON" : "Export JSON"}
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
              <p className="empty-state">No meeting selected.</p>
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
              {selectedMeeting ? (
                <StatusLine
                  icon={<ShieldCheck size={18} weight="regular" />}
                  label={analysisDisclosure?.label ?? "Summary unavailable"}
                  value={analysisDisclosure?.detail ?? "No selected meeting."}
                  tone={analysisDisclosure?.tone ?? "muted"}
                />
              ) : null}
            </div>
            <div className="settings-form" aria-label="Local settings">
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
              {settingsFeedback ? (
                <p className={`settings-feedback ${settingsFeedback.tone}`} role="status">
                  {settingsFeedback.message}
                </p>
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
