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
import { useMemo, useState } from "react";

import {
  DesktopSnapshot,
  getMockDesktopSnapshot,
  mapAnalysisDisclosure,
  mapDeleteState,
  mapExportState,
  mapModelStatus,
  mapPermissionState,
  mapRecordingState,
  searchMeetings,
  Tone,
} from "./commandAdapter";

import "./styles.css";

interface AppProps {
  snapshot?: DesktopSnapshot;
}

export default function App({ snapshot = getMockDesktopSnapshot() }: AppProps) {
  const [query, setQuery] = useState("");
  const [selectedMeetingId, setSelectedMeetingId] = useState(snapshot.selectedMeetingId);

  const meetings = useMemo(() => searchMeetings(snapshot.meetings, query), [snapshot.meetings, query]);
  const selectedMeeting = meetings.find((meeting) => meeting.id === selectedMeetingId) ?? meetings[0] ?? null;
  const commandUnavailable = snapshot.commandSurface.detail;
  const recording = {
    label: "Recording unavailable",
    tone: "muted" as Tone,
    detail: commandUnavailable,
  };
  const model = mapModelStatus(snapshot.model);

  const exportState = selectedMeeting
    ? mapExportState(selectedMeeting.exportState)
    : mapExportState({ state: "idle" });
  const deleteState = selectedMeeting
    ? mapDeleteState(selectedMeeting.deleteState)
    : mapDeleteState({ state: "idle" });
  const analysisDisclosure = selectedMeeting ? mapAnalysisDisclosure(selectedMeeting.analysis) : null;

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
            <StatusPill tone="muted" label="Preview shell" />
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
          <div className="strip-meta">
            <span>{snapshot.recording.storage_location.app_private_path}</span>
          </div>
        </section>

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

            {snapshot.loading ? <SkeletonList /> : null}

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

            {!snapshot.loading && query && meetings.length === 0 ? <p className="empty-state">No meetings match this search.</p> : null}
          </aside>

          <section className="detail-pane" aria-label="Meeting detail">
            {selectedMeeting ? (
              <>
                <div className="detail-header">
                  <div>
                    <p className="eyebrow">{selectedMeeting.startedAt}</p>
                    <h2>{selectedMeeting.title}</h2>
                  </div>
                  <StatusPill tone={selectedMeeting.transcriptState === "Ready" ? "ready" : "active"} label={selectedMeeting.transcriptState} />
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
                      title={commandUnavailable}
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
                    title={commandUnavailable}
                  >
                    <DownloadSimple size={16} weight="regular" />
                    Export JSON
                  </button>
                  <button
                    type="button"
                    className="button danger"
                    disabled
                    title={commandUnavailable}
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
            <StatusLine icon={<CheckCircle size={18} weight="regular" />} label={model.label} value={model.detail} tone={model.tone} />
            <StatusLine
              icon={<Microphone size={18} weight="regular" />}
              label={captureLabel(snapshot.capture.microphone)}
              value={captureDetail(snapshot.capture.microphone)}
              tone={captureTone(snapshot.capture.microphone)}
            />
            <StatusLine
              icon={<WarningDiamond size={18} weight="regular" />}
              label={captureLabel(snapshot.capture.systemAudio)}
              value={captureDetail(snapshot.capture.systemAudio)}
              tone={captureTone(snapshot.capture.systemAudio)}
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

function captureLabel(state: DesktopSnapshot["capture"]["microphone"]) {
  return mapPermissionState(state).label;
}

function captureDetail(state: DesktopSnapshot["capture"]["microphone"]) {
  return mapPermissionState(state).detail;
}

function captureTone(state: DesktopSnapshot["capture"]["microphone"]): Tone {
  return mapPermissionState(state).tone;
}

function formatTime(ms: number) {
  const seconds = Math.floor(ms / 1000);
  const minutes = Math.floor(seconds / 60);
  return `${minutes}:${String(seconds % 60).padStart(2, "0")}`;
}
