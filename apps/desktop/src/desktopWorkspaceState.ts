import { mapPermissionState } from "./commandAdapter";
import type { DesktopSnapshot, PersistedRawAudioRetentionPolicy, Tone } from "./commandAdapter";

export const ACTIVE_JOB_POLL_INTERVAL_MS = 250;

export type PendingCommand =
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

export interface SettingsFormState {
  whisperModelPath: string;
  ollamaBaseUrl: string;
  ollamaModel: string;
  rawAudioRetentionPolicy: PersistedRawAudioRetentionPolicy;
}

export function ollamaPullCommandModelLabel(pullCommand: string) {
  const parts = pullCommand.trim().split(/\s+/);
  if (parts[0] === "ollama" && parts[1] === "pull" && parts.length > 2) {
    return parts.slice(2).join(" ");
  }
  return pullCommand;
}

export function whisperSetupLabel(state: DesktopSnapshot["setupGuidance"]["whisper"]["state"]) {
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

export function whisperSetupTone(state: DesktopSnapshot["setupGuidance"]["whisper"]["state"]): Tone {
  if (state === "ReadablePath") {
    return "warn";
  }
  return "blocked";
}

export function ollamaSetupLabel(guidance: DesktopSnapshot["setupGuidance"]["ollama"]) {
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

export function ollamaSetupTone(guidance: DesktopSnapshot["setupGuidance"]["ollama"]): Tone {
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

export function ollamaSummaryBlocked(guidance: DesktopSnapshot["setupGuidance"]["ollama"]) {
  return (
    guidance.availability === "MissingModelAtLastTest" ||
    guidance.availability === "UnavailableAtLastTest"
  );
}

export function calendarContextLabel(context: DesktopSnapshot["calendarContext"]) {
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

export function calendarContextTone(context: DesktopSnapshot["calendarContext"]): Tone {
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

export function settingsFormFromSnapshot(snapshot: DesktopSnapshot): SettingsFormState {
  return {
    whisperModelPath: snapshot.settings.whisperModelPath,
    ollamaBaseUrl: snapshot.settings.ollamaBaseUrl,
    ollamaModel: snapshot.settings.ollamaModel,
    rawAudioRetentionPolicy: snapshot.settings.rawAudioRetentionPolicy,
  };
}

export function selectedTitleFromSnapshot(snapshot: DesktopSnapshot): string {
  const selected = snapshot.meetings.find((meeting) => meeting.id === snapshot.selectedMeetingId);
  return selected?.title ?? snapshot.meetings[0]?.title ?? "";
}

export function resolveSelectedMeetingId(snapshot: DesktopSnapshot, current: string | null): string | null {
  const backendSelected = snapshot.selectedMeetingId;
  if (backendSelected && snapshot.meetings.some((meeting) => meeting.id === backendSelected)) {
    return backendSelected;
  }
  if (current && snapshot.meetings.some((meeting) => meeting.id === current)) {
    return current;
  }
  return snapshot.meetings[0]?.id ?? null;
}

export function commandAllowedDuringBusy(
  next: Exclude<PendingCommand, null>,
  current: PendingCommand,
): boolean {
  return (
    (current === "transcribe" && next === "cancel-transcription") ||
    (current === "summary" && next === "cancel-summary")
  );
}

export function snapshotHasActiveCommandJob(snapshot: DesktopSnapshot): boolean {
  return (
    snapshot.transcriptionJob?.state === "Running" ||
    snapshot.transcriptionJob?.state === "CancelRequested" ||
    snapshot.summaryJob?.state === "Running" ||
    snapshot.summaryJob?.state === "CancelRequested"
  );
}

export function isActiveCommandJob(job: DesktopSnapshot["transcriptionJob"]): boolean {
  return job?.state === "Running" || job?.state === "CancelRequested";
}

export function isSelectedActiveCommandJob(
  job: DesktopSnapshot["transcriptionJob"],
  selectedMeetingId: string | undefined,
): boolean {
  return Boolean(job && selectedMeetingId && job.meetingId === selectedMeetingId && isActiveCommandJob(job));
}

export function isSelectedRetryableJob(
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

export function preserveCommandJobProgress(
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

export function captureLabel(state: DesktopSnapshot["capture"]["microphone"]) {
  return mapPermissionState(state).label;
}

export function captureDetail(state: DesktopSnapshot["capture"]["microphone"]) {
  return mapPermissionState(state).detail;
}

export function captureTone(state: DesktopSnapshot["capture"]["microphone"]): Tone {
  return mapPermissionState(state).tone;
}

export function commandErrorMessage(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }
  if (typeof error === "string") {
    return error;
  }
  return "Desktop command failed.";
}

export function formatTime(ms: number) {
  const seconds = Math.floor(ms / 1000);
  const minutes = Math.floor(seconds / 60);
  return `${minutes}:${String(seconds % 60).padStart(2, "0")}`;
}

export function formatEvidenceTimestamp(ms: number) {
  return new Date(ms).toISOString();
}

export function formatCalendarEventMetadata(event: DesktopSnapshot["calendarContext"]["upcomingEvents"][number]) {
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

export function formatMeetingCalendarAttachment(
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
