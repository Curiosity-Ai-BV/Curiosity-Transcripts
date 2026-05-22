import {
  CheckCircle,
  DownloadSimple,
  FileText,
  MagnifyingGlass,
  Microphone,
  ShieldCheck,
  Trash,
  WarningDiamond,
  Waveform,
} from "@phosphor-icons/react";
import { useEffect, useMemo, useState } from "react";

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
  searchMeetings,
  Tone,
  WhisperModelPathTestResult,
} from "./commandAdapter";

import "./styles.css";

interface AppProps {
  snapshot?: DesktopSnapshot;
  fetchCommand?: CommandFetcher;
}

type PendingCommand = "start" | "stop" | "transcribe" | "test-whisper" | "save-whisper" | "save-analysis" | null;

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
  const [selectedMeetingId, setSelectedMeetingId] = useState(initialSnapshot.selectedMeetingId);
  const [recordingTitle, setRecordingTitle] = useState("");
  const [settingsForm, setSettingsForm] = useState<SettingsFormState>(settingsFormFromSnapshot(initialSnapshot));
  const [settingsFeedback, setSettingsFeedback] = useState<SettingsFeedback | null>(null);
  const [pendingCommand, setPendingCommand] = useState<PendingCommand>(null);
  const [commandError, setCommandError] = useState<string | null>(null);

  useEffect(() => {
    if (snapshot) {
      setCurrentSnapshot(snapshot);
      setSettingsForm(settingsFormFromSnapshot(snapshot));
      setSettingsFeedback(null);
      setCommandError(null);
      setPendingCommand(null);
    }
  }, [snapshot]);

  const meetings = useMemo(() => searchMeetings(currentSnapshot.meetings, query), [currentSnapshot.meetings, query]);
  useEffect(() => {
    setSelectedMeetingId((current) => {
      return resolveSelectedMeetingId(currentSnapshot, current);
    });
  }, [currentSnapshot.meetings, currentSnapshot.selectedMeetingId]);

  const selectedMeeting = meetings.find((meeting) => meeting.id === selectedMeetingId) ?? meetings[0] ?? null;
  const commandUnavailable = currentSnapshot.commandSurface.detail;
  const commandSurfaceReady = Boolean(fetchCommand && commandUnavailable === connectedCommandSurface);
  const commandUnavailableTitle = commandSurfaceReady
    ? ""
    : fetchCommand || commandUnavailable.startsWith("Preview shell")
      ? commandUnavailable || "Desktop command surface is unavailable."
      : "Desktop command surface is unavailable in this runtime.";
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
  const shellLabel = commandUnavailable.startsWith("Preview shell") ? "Preview shell" : "Desktop shell";
  const startDisabled = !commandSurfaceReady || isRecordingActive || commandBusy;
  const stopDisabled = !commandSurfaceReady || !isRecordingActive || commandBusy;
  const transcribeDisabled = !commandSurfaceReady || !selectedMeeting || commandBusy;
  const recordingTitleDisabled = !commandSurfaceReady || isRecordingActive || commandBusy;
  const settingsDisabled = !commandSurfaceReady || commandBusy;

  const exportState = selectedMeeting
    ? mapExportState(selectedMeeting.exportState)
    : mapExportState({ state: "idle" });
  const deleteState = selectedMeeting
    ? mapDeleteState(selectedMeeting.deleteState)
    : mapDeleteState({ state: "idle" });
  const analysisDisclosure = selectedMeeting ? mapAnalysisDisclosure(selectedMeeting.analysis) : null;
  const summaryCommandUnavailable = "Summary command is not wired into the desktop shell yet.";
  const exportCommandUnavailable = "Export command is not wired into the desktop shell yet.";
  const deleteCommandUnavailable = "Delete command is not wired into the desktop shell yet.";

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

  async function runSettingsSnapshotCommand(
    pending: Exclude<PendingCommand, null>,
    command: string,
    args: Record<string, unknown>,
    successMessage: string,
  ) {
    if (!fetchCommand || !commandSurfaceReady || commandBusy) {
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
    if (!fetchCommand || !commandSurfaceReady || commandBusy) {
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
        : "Start microphone recording.";
  const stopButtonTitle = !commandSurfaceReady
    ? commandUnavailableTitle
    : commandBusy
      ? busyCommandTitle
      : isRecordingActive
        ? "Stop microphone recording."
        : "No active microphone recording to stop.";
  const transcribeButtonTitle = !commandSurfaceReady
    ? commandUnavailableTitle
    : commandBusy
      ? busyCommandTitle
      : selectedMeeting
        ? "Transcribe the selected meeting with the configured local Whisper model."
        : "Select a meeting before transcription.";

  return (
    <main className="app-shell">
      <section className="workspace" aria-label="Transcript workspace">
        <header className="topbar">
          <div>
            <p className="eyebrow">Curiosity Transcripts</p>
            <h1>Transcript workspace</h1>
          </div>
          <div className="topbar-status" aria-label="Workspace status">
            <StatusPill tone={recording.tone} label={recording.label} />
            <StatusPill tone={model.tone} label={model.label} />
            <StatusPill tone={transcription.tone} label={transcription.label} />
            <StatusPill tone={shellLabel === "Preview shell" ? "muted" : "ready"} label={shellLabel} />
          </div>
        </header>

        <section className="recording-strip" aria-label="Recording controls and status">
          <div className="strip-primary">
            <IconFrame tone={recording.tone}>
              <Waveform size={22} weight="regular" />
            </IconFrame>
            <div>
              <h2>Recording</h2>
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
                className="button"
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
        {commandError ? (
          <p role="alert" className="command-error">
            {commandError}
          </p>
        ) : null}

        <div className="content-grid">
          <aside className="meeting-pane" aria-label="Meetings">
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
                <div className="detail-header">
                  <div>
                    <p className="eyebrow">{selectedMeeting.startedAt}</p>
                    <h2>{selectedMeeting.title}</h2>
                  </div>
                  <div className="detail-header-actions">
                    <StatusPill tone={selectedMeeting.transcriptState === "Ready" ? "ready" : "active"} label={selectedMeeting.transcriptState} />
                    <button
                      type="button"
                      className="button"
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
                    </div>
                    <button
                      type="button"
                      className="button"
                      disabled
                      title={summaryCommandUnavailable}
                    >
                      <FileText size={16} weight="regular" />
                      Generate summary
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
                    disabled
                    title={exportCommandUnavailable}
                  >
                    <DownloadSimple size={16} weight="regular" />
                    Export JSON
                  </button>
                  <button
                    type="button"
                    className="button danger"
                    disabled
                    title={deleteCommandUnavailable}
                  >
                    <Trash size={16} weight="regular" />
                    Delete private data
                  </button>
                </div>
              </>
            ) : (
              <p className="empty-state">No meeting selected.</p>
            )}
          </section>

          <aside className="settings-pane" aria-label="Settings and model status">
            <h2>Settings</h2>
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
                  disabled={settingsDisabled}
                />
              </label>
              <div className="settings-buttons">
                <button
                  type="button"
                  className="button"
                  disabled={settingsDisabled}
                  title={commandSurfaceReady ? "Test the configured Whisper path." : commandUnavailableTitle}
                  onClick={testWhisperModelPath}
                >
                  {pendingCommand === "test-whisper" ? "Testing path" : "Test path"}
                </button>
                <button
                  type="button"
                  className="button"
                  disabled={settingsDisabled}
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
                  onChange={(event) =>
                    setSettingsForm((current) => ({ ...current, ollamaBaseUrl: event.target.value }))
                  }
                  placeholder="http://127.0.0.1:11434"
                  disabled={settingsDisabled}
                />
              </label>
              <label className="settings-field" htmlFor="ollama-model">
                <span>Ollama model</span>
                <input
                  id="ollama-model"
                  value={settingsForm.ollamaModel}
                  onChange={(event) =>
                    setSettingsForm((current) => ({ ...current, ollamaModel: event.target.value }))
                  }
                  placeholder="qwen3.6:27b"
                  disabled={settingsDisabled}
                />
              </label>
              <button
                type="button"
                className="button"
                disabled={settingsDisabled}
                title={commandSurfaceReady ? "Save local analysis settings." : commandUnavailableTitle}
                onClick={saveAnalysisSettings}
              >
                {pendingCommand === "save-analysis" ? "Saving analysis" : "Save analysis"}
              </button>
              {settingsFeedback ? (
                <p className={`settings-feedback ${settingsFeedback.tone}`} role="status">
                  {settingsFeedback.message}
                </p>
              ) : null}
            </div>
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
