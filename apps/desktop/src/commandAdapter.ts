export type CommandRecordingState =
  | "Idle"
  | "Recording"
  | "Paused"
  | "Stopping"
  | "Interrupted"
  | "Recovering"
  | "Complete";

export type AppPermissionState =
  | "Ready"
  | "MicrophoneDenied"
  | "SystemAudioDenied"
  | "MicrophoneUnavailable"
  | "SystemAudioUnavailable";
export type RawAudioRetentionPolicy = "Retain" | "DeleteAfterTranscription" | "NeverSave";
export type PersistedRawAudioRetentionPolicy = Exclude<RawAudioRetentionPolicy, "NeverSave">;
export type Tone = "ready" | "active" | "warn" | "blocked" | "muted";
export type ExportFormat = "json" | "markdown" | "srt";

const RAW_AUDIO_RETENTION_POLICIES = ["Retain", "DeleteAfterTranscription", "NeverSave"] as const;
const PERSISTED_RAW_AUDIO_RETENTION_POLICIES = ["Retain", "DeleteAfterTranscription"] as const;

export interface CommandRecordingDto {
  meeting_id: string;
  recording_id: string | null;
  state: CommandRecordingState;
  permission_state: AppPermissionState;
  storage_location: { app_private_path: string };
  raw_audio_retention: RawAudioRetentionPolicy;
  recoverable: boolean;
  recovery_action: string;
}

export interface StatusView {
  label: string;
  tone: Tone;
  detail: string;
}

export interface CommandSurfaceState {
  ready: boolean;
  detail: string;
}

export interface ModelStatus {
  kind: "ready" | "missing" | "untested" | "transcribing";
  configuredPath: string;
}

export type WhisperSetupState = "MissingPath" | "UnreadablePath" | "ReadablePath";
export type OllamaSetupState = "ConfiguredNotChecked" | "InvalidLocalConfiguration";
export type OllamaAvailabilityState =
  | "UnknownUntilTest"
  | "AvailableAtLastTest"
  | "MissingModelAtLastTest"
  | "UnavailableAtLastTest";

export interface WhisperSetupGuidance {
  state: WhisperSetupState;
  configuredPath: string;
  message: string;
  setupGuidance: string;
  compatibilityNote: string;
  lastPathTest: WhisperPathTestEvidence | null;
  lastSuccessfulTranscription: WhisperTranscriptionCompatibilityEvidence | null;
}

export interface OllamaSetupGuidance {
  state: OllamaSetupState;
  baseUrl: string;
  model: string;
  availability: OllamaAvailabilityState;
  message: string;
  setupGuidance: string;
  lastConnectionTest: OllamaConnectionTestEvidence | null;
}

export interface FirstRunSetupGuidance {
  whisper: WhisperSetupGuidance;
  ollama: OllamaSetupGuidance;
}

export type CalendarPermissionState = "NotRequested" | "Granted" | "Denied" | "Unavailable";
export type CalendarAvailabilityState = "Unavailable" | "PermissionRequired" | "Ready";
export type CalendarEventPrivacy = "Unknown" | "Public" | "Private";
export type CalendarEventOverlapState = "None" | "Ambiguous" | "Overlapping";

export interface CalendarContextEvent {
  id: string;
  title: string;
  calendarTitle: string;
  startsAtMs: number;
  endsAtMs: number;
  isAllDay: boolean;
  isRecurring: boolean;
  privacy: CalendarEventPrivacy;
  overlapState: CalendarEventOverlapState;
  attachable: boolean;
  safetyNote: string;
}

export interface CalendarContext {
  source: "AppleCalendar";
  permissionState: CalendarPermissionState;
  availabilityState: CalendarAvailabilityState;
  message: string;
  setupGuidance: string;
  upcomingEvents: CalendarContextEvent[];
  autoStartEnabled: false;
}

export interface MeetingCalendarAttachment {
  source: "AppleCalendar";
  eventId: string;
  eventTitle: string;
  calendarTitle: string;
  startsAtMs: number;
  endsAtMs: number;
  privacy: CalendarEventPrivacy;
  privacyConfirmed: boolean;
  attachedAtMs: number;
}

export interface AppSettings {
  whisperModelPath: string;
  ollamaBaseUrl: string;
  ollamaModel: string;
  exportDirectory: string | null;
  rawAudioRetentionPolicy: PersistedRawAudioRetentionPolicy;
}

interface WhisperModelPathTestBase {
  message: string;
  setupGuidance: string;
}

export interface WhisperPathTestEvidence {
  testedPath: string;
  testedAtMs: number;
  state: "Valid" | "Invalid";
  fileSizeBytes: number | null;
  sha256: string | null;
  failureDetail: string | null;
}

export interface WhisperTranscriptionCompatibilityEvidence {
  modelPath: string;
  usedAtMs: number;
  provider: string;
  modelName: string;
  meetingId: string;
  modelRunId: string;
  transcriptVersionId: string;
  segmentCount: number;
  fileSizeBytes: number;
  modifiedAtMs: number;
}

export type WhisperModelPathTestResult =
  | (WhisperModelPathTestBase & {
      state: "Valid";
      fileSizeBytes: number;
      sha256: string;
    })
  | (WhisperModelPathTestBase & {
      state: "Invalid";
      fileSizeBytes?: undefined;
      sha256?: undefined;
    });

export interface OllamaConnectionTestResult {
  state: "Available" | "Unavailable";
  message: string;
  setupGuidance: string;
  selectedLocalModelTag: string | null;
  installedLocalModels: string[] | null;
  pullCommand: string | null;
}

export interface OllamaConnectionTestEvidence {
  baseUrl: string;
  requestedModel: string;
  testedAtMs: number;
  state: "Available" | "Unavailable";
  selectedLocalModelTag: string | null;
  installedLocalModels: string[] | null;
  pullCommand: string | null;
  failureDetail: string | null;
}

export interface ExportCommandState {
  state: "idle" | "exporting" | "exported" | "failed";
  meetingId?: string | null;
  format?: ExportFormat | null;
  path?: string | null;
  message?: string | null;
}

export interface DeleteCommandState {
  state: "idle" | "deleting" | "deleted" | "failed";
  meetingId?: string | null;
  deletedPrivateArtifacts?: string[];
  skippedPrivateArtifacts?: string[];
  remainingExports?: string[];
  message?: string | null;
}

export interface MeetingSearchResult {
  meeting_id: string;
  title: string;
}

export interface AnalysisDisclosureState {
  provider: string;
  modelName: string;
  networkUsed: boolean;
  disclosureRequired: boolean;
  disclosureConfirmed: boolean;
  summary?: string | null;
  createdAtMs?: number | null;
  promptTemplateVersion?: string | null;
}

export interface CommandFailureView {
  code: string;
  message: string;
  setupGuidance: string;
}

export interface AnalysisCommandView {
  meetingId: string;
  state: "Complete" | "Failed";
  analysis?: {
    provider: string;
    modelName: string;
    networkUsed: boolean;
    summary: string;
  } | null;
  failure?: CommandFailureView | null;
}

export interface TranscriptionCommandView {
  meetingId: string;
  state: "Complete" | "Failed";
  failure?: CommandFailureView | null;
}

export interface TranscriptSegment {
  id: string;
  startMs: number;
  endMs: number;
  text: string;
  originalText: string | null;
  sourceChannel: string;
  modelRunId: string;
  transcriptVersionId: string;
}

export interface MeetingView {
  id: string;
  title: string;
  startedAt: string;
  duration: string;
  status: string;
  transcriptState: "Ready" | "Transcribing" | "Unavailable";
  transcriptText: string;
  segments: TranscriptSegment[];
  privacy: {
    storageLabel: string;
    storagePath: string;
    rawAudioRetention: RawAudioRetentionPolicy;
    localOnly: boolean;
  };
  exportState: ExportCommandState;
  deleteState: DeleteCommandState;
  calendarAttachment: MeetingCalendarAttachment | null;
  analysis: AnalysisDisclosureState | null;
}

export interface CaptureStatus {
  microphone: AppPermissionState;
  systemAudio: AppPermissionState;
}

export interface CommandJobView {
  id: string;
  kind: "Transcription" | "Summary";
  meetingId: string;
  state: "Running" | "CancelRequested" | "Complete" | "Failed" | "Recovery" | "Retry" | "Canceled";
  cancelRequested: boolean;
  startedAtMs: number;
  lastError?: string | null;
}

export interface DesktopSnapshot {
  loading: boolean;
  commandSurface: CommandSurfaceState;
  meetings: MeetingView[];
  selectedMeetingId: string | null;
  recording: CommandRecordingDto;
  model: ModelStatus;
  setupGuidance: FirstRunSetupGuidance;
  calendarContext: CalendarContext;
  settings: AppSettings;
  capture: CaptureStatus;
  transcription: TranscriptionCommandView | null;
  transcriptionJob: CommandJobView | null;
  exportCommand: ExportCommandState;
  deleteCommand: DeleteCommandState;
  analysisCommand: AnalysisCommandView | null;
  summaryJob: CommandJobView | null;
}

export type CommandFetcher = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;

export interface DesktopCommandFacade {
  desktopSnapshot(): Promise<DesktopSnapshot>;
  searchMeetings(args: { query: string }): Promise<MeetingSearchResult[]>;
  startRecording(args?: { title?: string }): Promise<DesktopSnapshot>;
  importAudioFile(args: { sourcePath: string; title?: string }): Promise<DesktopSnapshot>;
  stopRecording(): Promise<DesktopSnapshot>;
  transcribeMeeting(args: { meetingId: string }): Promise<DesktopSnapshot>;
  correctTranscriptSegment(args: {
    meetingId: string;
    segmentId: string;
    correctedText: string;
    editedAtMs: number;
  }): Promise<DesktopSnapshot>;
  cancelTranscription(args: { jobId: string }): Promise<DesktopSnapshot>;
  renameMeeting(args: { meetingId: string; title: string }): Promise<DesktopSnapshot>;
  exportMeeting(args: { meetingId: string; format: ExportFormat }): Promise<DesktopSnapshot>;
  exportMeetingJson(args: { meetingId: string }): Promise<DesktopSnapshot>;
  deleteMeeting(args: { meetingId: string }): Promise<DesktopSnapshot>;
  generateSummary(args: { meetingId: string }): Promise<DesktopSnapshot>;
  cancelSummary(args: { jobId: string }): Promise<DesktopSnapshot>;
  saveWhisperModelPath(args: { whisperModelPath: string }): Promise<DesktopSnapshot>;
  saveAnalysisSettings(args: { ollamaBaseUrl: string; ollamaModel: string }): Promise<DesktopSnapshot>;
  saveRawAudioRetentionPolicy(args: { rawAudioRetentionPolicy: PersistedRawAudioRetentionPolicy }): Promise<DesktopSnapshot>;
  requestAppleCalendarAccess(): Promise<DesktopSnapshot>;
  attachCalendarEventContext(args: {
    meetingId: string;
    eventId: string;
    privacyConfirmed: boolean;
  }): Promise<DesktopSnapshot>;
  testWhisperModelPath(args: { path: string }): Promise<WhisperModelPathTestResult>;
  testOllamaConnection(args: { baseUrl: string; model: string }): Promise<OllamaConnectionTestResult>;
}

interface LoadDesktopSnapshotOptions {
  fetchCommand?: CommandFetcher;
  previewFallback?: boolean;
}

export async function loadDesktopSnapshot({
  fetchCommand = getDesktopCommandFetcher(),
  previewFallback = !isTauriRuntime(),
}: LoadDesktopSnapshotOptions = {}): Promise<DesktopSnapshot> {
  if (fetchCommand) {
    const snapshot = await fetchCommand<unknown>("desktop_snapshot");
    assertDesktopSnapshotContract(snapshot);
    return snapshot;
  }
  if (previewFallback) {
    return getMockDesktopSnapshot();
  }
  throw new Error("Tauri command surface is unavailable");
}

export function assertDesktopSnapshotContract(value: unknown): asserts value is DesktopSnapshot {
  for (const path of REQUIRED_DESKTOP_SNAPSHOT_PATHS) {
    requireContractPath(value, path, "desktop_snapshot");
  }

  const root = requireContractRecord(value, "desktop_snapshot");
  requireBoolean(root.loading, "desktop_snapshot.loading");
  const commandSurface = requireContractRecord(root.commandSurface, "desktop_snapshot.commandSurface");
  requireBoolean(commandSurface.ready, "desktop_snapshot.commandSurface.ready");
  requireString(commandSurface.detail, "desktop_snapshot.commandSurface.detail");
  requireNullableString(root.selectedMeetingId, "desktop_snapshot.selectedMeetingId");

  requireContractArray(root.meetings, "desktop_snapshot.meetings").forEach((meeting, index) => {
    const meetingPath = `desktop_snapshot.meetings[${index}]`;
    for (const path of REQUIRED_MEETING_PATHS) {
      requireContractPath(meeting, path, meetingPath);
    }

    const meetingRecord = requireContractRecord(meeting, meetingPath);
    requireString(meetingRecord.id, `${meetingPath}.id`);
    requireString(meetingRecord.title, `${meetingPath}.title`);
    requireString(meetingRecord.startedAt, `${meetingPath}.startedAt`);
    requireString(meetingRecord.duration, `${meetingPath}.duration`);
    requireString(meetingRecord.status, `${meetingPath}.status`);
    requireEnum(meetingRecord.transcriptState, ["Ready", "Transcribing", "Unavailable"], `${meetingPath}.transcriptState`);
    requireString(meetingRecord.transcriptText, `${meetingPath}.transcriptText`);
    const privacy = requireContractRecord(meetingRecord.privacy, `${meetingPath}.privacy`);
    requireString(privacy.storageLabel, `${meetingPath}.privacy.storageLabel`);
    requireString(privacy.storagePath, `${meetingPath}.privacy.storagePath`);
    requireEnum(
      privacy.rawAudioRetention,
      RAW_AUDIO_RETENTION_POLICIES,
      `${meetingPath}.privacy.rawAudioRetention`,
    );
    requireBoolean(privacy.localOnly, `${meetingPath}.privacy.localOnly`);
    validateExportCommandState(meetingRecord.exportState, `${meetingPath}.exportState`);
    validateDeleteCommandState(meetingRecord.deleteState, `${meetingPath}.deleteState`);
    validateMeetingCalendarAttachment(meetingRecord.calendarAttachment, `${meetingPath}.calendarAttachment`);
    requireContractArray(meetingRecord.segments, `${meetingPath}.segments`).forEach(
      (segment, segmentIndex) => {
        const segmentPath = `${meetingPath}.segments[${segmentIndex}]`;
        for (const path of REQUIRED_SEGMENT_PATHS) {
          requireContractPath(segment, path, segmentPath);
        }
        const segmentRecord = requireContractRecord(segment, segmentPath);
        requireString(segmentRecord.id, `${segmentPath}.id`);
        requireNumber(segmentRecord.startMs, `${segmentPath}.startMs`);
        requireNumber(segmentRecord.endMs, `${segmentPath}.endMs`);
        requireString(segmentRecord.text, `${segmentPath}.text`);
        requireNullableString(segmentRecord.originalText, `${segmentPath}.originalText`);
        requireString(segmentRecord.sourceChannel, `${segmentPath}.sourceChannel`);
        requireString(segmentRecord.modelRunId, `${segmentPath}.modelRunId`);
        requireString(segmentRecord.transcriptVersionId, `${segmentPath}.transcriptVersionId`);
      },
    );
    validateAnalysisDisclosureState(meetingRecord.analysis, `${meetingPath}.analysis`);
  });

  const recording = requireContractRecord(root.recording, "desktop_snapshot.recording");
  requireString(recording.meeting_id, "desktop_snapshot.recording.meeting_id");
  requireNullableString(recording.recording_id, "desktop_snapshot.recording.recording_id");
  requireEnum(
    recording.state,
    ["Idle", "Recording", "Paused", "Stopping", "Interrupted", "Recovering", "Complete"],
    "desktop_snapshot.recording.state",
  );
  requireEnum(
    recording.permission_state,
    ["Ready", "MicrophoneDenied", "SystemAudioDenied", "MicrophoneUnavailable", "SystemAudioUnavailable"],
    "desktop_snapshot.recording.permission_state",
  );
  const storageLocation = requireContractRecord(recording.storage_location, "desktop_snapshot.recording.storage_location");
  requireString(storageLocation.app_private_path, "desktop_snapshot.recording.storage_location.app_private_path");
  requireEnum(
    recording.raw_audio_retention,
    RAW_AUDIO_RETENTION_POLICIES,
    "desktop_snapshot.recording.raw_audio_retention",
  );
  requireBoolean(recording.recoverable, "desktop_snapshot.recording.recoverable");
  requireString(recording.recovery_action, "desktop_snapshot.recording.recovery_action");

  const model = requireContractRecord(root.model, "desktop_snapshot.model");
  const modelKind = requireEnum(
    model.kind,
    ["ready", "missing", "untested", "transcribing"],
    "desktop_snapshot.model.kind",
  );
  const configuredModelPath = requireString(model.configuredPath, "desktop_snapshot.model.configuredPath");

  validateFirstRunSetupGuidance(root.setupGuidance, "desktop_snapshot.setupGuidance", configuredModelPath);
  validateWhisperModelReadinessEvidence(modelKind, configuredModelPath, root.setupGuidance);
  validateCalendarContext(root.calendarContext, "desktop_snapshot.calendarContext");

  const settings = requireContractRecord(root.settings, "desktop_snapshot.settings");
  requireString(settings.whisperModelPath, "desktop_snapshot.settings.whisperModelPath");
  requireString(settings.ollamaBaseUrl, "desktop_snapshot.settings.ollamaBaseUrl");
  requireString(settings.ollamaModel, "desktop_snapshot.settings.ollamaModel");
  requireNullableString(settings.exportDirectory, "desktop_snapshot.settings.exportDirectory");
  requireEnum(
    settings.rawAudioRetentionPolicy,
    PERSISTED_RAW_AUDIO_RETENTION_POLICIES,
    "desktop_snapshot.settings.rawAudioRetentionPolicy",
  );

  const capture = requireContractRecord(root.capture, "desktop_snapshot.capture");
  requireEnum(
    capture.microphone,
    ["Ready", "MicrophoneDenied", "SystemAudioDenied", "MicrophoneUnavailable", "SystemAudioUnavailable"],
    "desktop_snapshot.capture.microphone",
  );
  requireEnum(
    capture.systemAudio,
    ["Ready", "MicrophoneDenied", "SystemAudioDenied", "MicrophoneUnavailable", "SystemAudioUnavailable"],
    "desktop_snapshot.capture.systemAudio",
  );
  validateExportCommandState(root.exportCommand, "desktop_snapshot.exportCommand");
  validateDeleteCommandState(root.deleteCommand, "desktop_snapshot.deleteCommand");

  validateTranscriptionCommandView(root.transcription, "desktop_snapshot.transcription");
  validateCommandJobView(root.transcriptionJob, "desktop_snapshot.transcriptionJob");
  validateAnalysisCommandView(root.analysisCommand, "desktop_snapshot.analysisCommand");
  validateCommandJobView(root.summaryJob, "desktop_snapshot.summaryJob");
}

export function isTauriRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export function mapRecordingState(
  dto: Pick<
    CommandRecordingDto,
    | "state"
    | "permission_state"
    | "storage_location"
    | "raw_audio_retention"
    | "recoverable"
    | "recovery_action"
  >,
): StatusView {
  if (dto.state === "Idle") {
    return {
      label: "Recording idle",
      tone: "muted",
      detail: dto.recovery_action || "Start a desktop recording when you are ready.",
    };
  }
  if (dto.permission_state !== "Ready") {
    const permission = mapPermissionState(dto.permission_state);
    return {
      ...permission,
      detail: dto.recovery_action || permission.detail,
    };
  }
  if (dto.state === "Interrupted") {
    return {
      label: dto.recoverable ? "Recoverable interruption" : "Interrupted",
      tone: dto.recoverable ? "warn" : "blocked",
      detail: dto.recovery_action || "Recording stopped before a complete artifact was saved.",
    };
  }
  if (dto.state === "Recovering") {
    return {
      label: "Recovering",
      tone: "warn",
      detail: "Recovering a private audio artifact from local evidence.",
    };
  }
  if (dto.state === "Stopping") {
    return {
      label: "Stopping",
      tone: "warn",
      detail: "Finalizing the local recording session.",
    };
  }
  if (dto.state === "Complete") {
    return {
      label: "Recorded",
      tone: "ready",
      detail: dto.recovery_action || "Local desktop WAV artifacts are saved.",
    };
  }
  return {
    label: dto.state,
    tone: dto.state === "Recording" ? "active" : "muted",
    detail: retentionDetail(dto.raw_audio_retention),
  };
}

export function mapTranscriptionState(state: TranscriptionCommandView | null): StatusView {
  if (!state) {
    return {
      label: "Transcription idle",
      tone: "muted",
      detail: "Record a meeting, then transcribe it with a local Whisper model.",
    };
  }
  if (state.state === "Failed") {
    return {
      label: "Transcription failed",
      tone: "blocked",
      detail: state.failure?.message || "Local transcription failed.",
    };
  }
  return {
    label: "Transcript ready",
    tone: "ready",
    detail: "Local Whisper transcript persisted in private storage.",
  };
}

export function mapCommandJobState(job: CommandJobView): StatusView {
  const kind = job.kind === "Transcription" ? "Transcription" : "Summary";
  const retryGuidance = `Retry this ${kind.toLowerCase()} job when you are ready.`;
  if (job.state === "CancelRequested") {
    return {
      label: `${kind} cancel requested`,
      tone: "warn",
      detail: `${job.meetingId} / ${job.id}`,
    };
  }
  if (job.state === "Running") {
    return {
      label: `${kind} running`,
      tone: "active",
      detail: `${job.meetingId} / ${job.id}`,
    };
  }
  if (job.state === "Failed") {
    return {
      label: `${kind} failed`,
      tone: "blocked",
      detail: job.lastError ?? `${job.meetingId} / ${job.id}`,
    };
  }
  if (job.state === "Recovery") {
    return {
      label: `${kind} recovered`,
      tone: "warn",
      detail: job.lastError ?? retryGuidance,
    };
  }
  if (job.state === "Retry") {
    return {
      label: `${kind} retryable`,
      tone: "warn",
      detail: job.lastError ?? retryGuidance,
    };
  }
  if (job.state === "Canceled") {
    return {
      label: `${kind} canceled`,
      tone: "warn",
      detail: `${job.meetingId} / ${job.id}`,
    };
  }
  return {
    label: `${kind} complete`,
    tone: "ready",
    detail: `${job.meetingId} / ${job.id}`,
  };
}

export function mapPermissionState(state: AppPermissionState): StatusView {
  if (state === "MicrophoneDenied") {
    return {
      label: "Microphone denied",
      tone: "blocked",
      detail: "Open macOS Privacy & Security and allow microphone access.",
    };
  }
  if (state === "SystemAudioDenied") {
    return {
      label: "System audio denied",
      tone: "blocked",
      detail: "Allow Screen Recording before mixed/system capture.",
    };
  }
  if (state === "MicrophoneUnavailable") {
    return {
      label: "Microphone unavailable",
      tone: "blocked",
      detail: "Connect or select a macOS input device before recording.",
    };
  }
  if (state === "SystemAudioUnavailable") {
    return {
      label: "System audio unavailable",
      tone: "blocked",
      detail: "Run the ScreenCaptureKit desktop backend and allow Screen Recording before recording.",
    };
  }
  return {
    label: "Ready",
    tone: "ready",
    detail: "Capture permissions are ready.",
  };
}

export function mapModelStatus(model: ModelStatus): StatusView {
  if (model.kind === "missing") {
    return {
      label: "Whisper model missing",
      tone: "blocked",
      detail: "Choose a local model path before transcription.",
    };
  }
  if (model.kind === "untested") {
    return {
      label: "Whisper path untested",
      tone: "blocked",
      detail: "Run Test path for the saved model file before transcription.",
    };
  }
  if (model.kind === "transcribing") {
    return {
      label: "Transcribing",
      tone: "active",
      detail: model.configuredPath || "Local transcription is running.",
    };
  }
  return {
    label: "Ready",
    tone: "ready",
    detail: model.configuredPath || "Local Whisper model path configured.",
  };
}

export function searchMeetings(meetings: MeetingView[], query: string): MeetingView[] {
  const normalized = query.trim().toLowerCase();
  if (!normalized) {
    return meetings;
  }
  return meetings.filter((meeting) => {
    return (
      meeting.title.toLowerCase().includes(normalized) ||
      meeting.transcriptText.toLowerCase().includes(normalized)
    );
  });
}

export function mapExportState(state: ExportCommandState): StatusView {
  const formatLabel = exportFormatLabel(state.format);
  if (state.state === "exported") {
    return {
      label: `${formatLabel} exported`,
      tone: "ready",
      detail: state.path || "Export path recorded.",
    };
  }
  if (state.state === "exporting") {
    return {
      label: "Exporting",
      tone: "active",
      detail: `Writing a user-requested ${formatLabel} export.`,
    };
  }
  if (state.state === "failed") {
    return {
      label: "Export failed",
      tone: "blocked",
      detail: state.message || "The export command returned a failure.",
    };
  }
  return {
    label: "Not exported",
    tone: "muted",
    detail: "No export has been requested for this meeting.",
  };
}

export function exportFormatLabel(format: ExportFormat | null | undefined): string {
  if (format === "markdown") {
    return "Markdown";
  }
  if (format === "srt") {
    return "SRT";
  }
  return "JSON";
}

export function mapDeleteState(state: DeleteCommandState): StatusView {
  if (state.state === "deleted") {
    const deleted = state.deletedPrivateArtifacts?.length ?? 0;
    const skipped = state.skippedPrivateArtifacts?.length ?? 0;
    const remaining = state.remainingExports?.length ?? 0;
    const remainingExportsDetail = `${remaining} exported file${remaining === 1 ? "" : "s"} ${remaining === 1 ? "remains" : "remain"} outside app control.`;
    if (skipped > 0) {
      return {
        label: "Cleanup incomplete",
        tone: "warn",
        detail: `${deleted} private artifact${deleted === 1 ? "" : "s"} removed. Cleanup incomplete: ${skipped} private artifact${skipped === 1 ? "" : "s"} could not be removed. ${remainingExportsDetail}`,
      };
    }
    return {
      label: "Private artifacts deleted",
      tone: remaining > 0 ? "warn" : "ready",
      detail: `${deleted} private artifact${deleted === 1 ? "" : "s"} removed. ${remainingExportsDetail}`,
    };
  }
  if (state.state === "deleting") {
    return {
      label: "Deleting",
      tone: "active",
      detail: "Removing local private artifacts controlled by the app.",
    };
  }
  if (state.state === "failed") {
    return {
      label: "Delete failed",
      tone: "blocked",
      detail: state.message || "The delete command returned a failure.",
    };
  }
  return {
    label: "Not deleted",
    tone: "muted",
    detail: "Private artifacts are still retained.",
  };
}

export function mapRawAudioRetention(policy: RawAudioRetentionPolicy): StatusView {
  if (policy === "DeleteAfterTranscription") {
    return {
      label: "Delete after transcription",
      tone: "ready",
      detail: retentionDetail(policy),
    };
  }
  if (policy === "NeverSave") {
    return {
      label: "Raw audio not saved",
      tone: "ready",
      detail: retentionDetail(policy),
    };
  }
  return {
    label: "Raw audio retained",
    tone: "warn",
    detail: retentionDetail(policy),
  };
}

export function mapLocalProcessingState(localOnly: boolean): StatusView {
  if (localOnly) {
    return {
      label: "Stayed local",
      tone: "ready",
      detail: "No hosted processing recorded for this meeting.",
    };
  }
  return {
    label: "Hosted processing used",
    tone: "warn",
    detail: "Transcript/summary data may have left this device.",
  };
}

export function mapAnalysisDisclosure(state: AnalysisDisclosureState | null): StatusView {
  if (!state) {
    return {
      label: "No summary",
      tone: "muted",
      detail: "Generate a local Ollama summary after a transcript is ready.",
    };
  }
  if (state.networkUsed && state.disclosureRequired && !state.disclosureConfirmed) {
    return {
      label: "Hosted summary gated",
      tone: "blocked",
      detail: "Select a key and confirm transcript data disclosure before sending anything.",
    };
  }
  if (state.networkUsed) {
    return {
      label: "Hosted summary",
      tone: "warn",
      detail: `${state.provider} / ${state.modelName}. Transcript data may leave this device.`,
    };
  }
  return {
    label: "Local summary",
    tone: "ready",
    detail: `${state.provider} / ${state.modelName}. Transcript stays on this device.`,
  };
}

export function getMockDesktopSnapshot(variant: "default" | "state-matrix" = "default"): DesktopSnapshot {
  const meetings = [
    meeting({
      id: "circuit-review",
      title: "Circuit Review",
      startedAt: "Thu 09:12",
      duration: "38m",
      transcriptState: "Ready",
      transcriptText:
        "We decided to keep raw audio retention visible and require explicit exports for files outside app control.",
      segments: [
        segment("segment-1", 0, 8400, "We decided to keep raw audio retention visible.", "Mixed"),
        segment(
          "segment-2",
          8400,
          21400,
          "Exports should show when files remain outside app control.",
          "Mixed",
        ),
      ],
      analysis: null,
    }),
    meeting({
      id: "design-standup",
      title: "Design Standup",
      startedAt: "Wed 14:35",
      duration: "22m",
      transcriptState: "Transcribing",
      transcriptText: "The standup covered narrow layout density and settings copy.",
      segments: [segment("segment-3", 0, 9300, "The standup covered narrow layout density.", "Imported")],
      analysis: null,
    }),
  ];

  return {
    loading: variant === "state-matrix",
    commandSurface: {
      ready: false,
      detail: "Preview shell: backend command wiring is not connected in this browser/dev fixture.",
    },
    meetings,
    selectedMeetingId: "circuit-review",
    recording: {
      meeting_id: "circuit-review",
      recording_id: "recording-circuit-review",
      state: variant === "state-matrix" ? "Paused" : "Recording",
      permission_state: "Ready",
      storage_location: { app_private_path: "meetings/circuit-review/audio" },
      raw_audio_retention: "Retain",
      recoverable: false,
      recovery_action: "",
    },
    model:
      variant === "state-matrix"
        ? { kind: "transcribing", configuredPath: "~/Library/Application Support/Curiosity/models/base.en.bin" }
        : { kind: "missing", configuredPath: "" },
    setupGuidance:
      variant === "state-matrix"
        ? {
            whisper: {
              state: "ReadablePath",
              configuredPath: "~/Library/Application Support/Curiosity/models/base.en.bin",
              message: "Whisper model path is readable; compatibility is not verified.",
              setupGuidance:
                "Use Test path for file evidence, then transcribe a sample to verify compatibility.",
              compatibilityNote: "Readability does not prove model compatibility.",
              lastPathTest: null,
              lastSuccessfulTranscription: null,
            },
            ollama: {
              state: "ConfiguredNotChecked",
              baseUrl: "http://127.0.0.1:11434",
              model: "qwen3.6:27b",
              availability: "UnknownUntilTest",
              message: "Ollama is configured for a local loopback URL and model.",
              setupGuidance:
                "Start Ollama manually, install the selected local model if needed, then run Test Ollama. Availability is unknown until Test Ollama runs.",
              lastConnectionTest: null,
            },
          }
        : {
            whisper: {
              state: "MissingPath",
              configuredPath: "",
              message: "No Whisper model path is configured.",
              setupGuidance: "Enter a local Whisper model path in Settings, save it, then use Test path.",
              compatibilityNote: "Readability does not prove model compatibility.",
              lastPathTest: null,
              lastSuccessfulTranscription: null,
            },
            ollama: {
              state: "ConfiguredNotChecked",
              baseUrl: "http://127.0.0.1:11434",
              model: "qwen3.6:27b",
              availability: "UnknownUntilTest",
              message: "Ollama is configured for a local loopback URL and model.",
              setupGuidance:
                "Start Ollama manually, install the selected local model if needed, then run Test Ollama. Availability is unknown until Test Ollama runs.",
              lastConnectionTest: null,
            },
          },
    calendarContext: {
      source: "AppleCalendar",
      permissionState: "NotRequested",
      availabilityState: "PermissionRequired",
      message: "Apple Calendar permission has not been requested.",
      setupGuidance:
        "Use Request calendar access when you want Curiosity to read upcoming local Calendar events. Calendar events never start recordings automatically.",
      upcomingEvents: [],
      autoStartEnabled: false,
    },
    settings: {
      whisperModelPath:
        variant === "state-matrix" ? "~/Library/Application Support/Curiosity/models/base.en.bin" : "",
      ollamaBaseUrl: "http://127.0.0.1:11434",
      ollamaModel: "qwen3.6:27b",
      exportDirectory: null,
      rawAudioRetentionPolicy: "Retain",
    },
    capture:
      variant === "state-matrix"
        ? {
            microphone: "MicrophoneDenied",
            systemAudio: "SystemAudioUnavailable",
          }
        : {
            microphone: "Ready",
            systemAudio: "SystemAudioDenied",
          },
    transcription: null,
    transcriptionJob: null,
    exportCommand: {
      state: "idle",
    },
    deleteCommand: {
      state: "idle",
    },
    analysisCommand: null,
    summaryJob: null,
  };
}

export function getUnavailableDesktopSnapshot(detail: string): DesktopSnapshot {
  return {
    loading: false,
    commandSurface: {
      ready: false,
      detail,
    },
    meetings: [],
    selectedMeetingId: null,
    recording: {
      meeting_id: "",
      recording_id: null,
      state: "Interrupted",
      permission_state: "MicrophoneUnavailable",
      storage_location: { app_private_path: "" },
      raw_audio_retention: "Retain",
      recoverable: false,
      recovery_action: "Load the desktop command surface before recording.",
    },
    model: { kind: "missing", configuredPath: "" },
    setupGuidance: {
      whisper: {
        state: "MissingPath",
        configuredPath: "",
        message: "No Whisper model path is configured.",
        setupGuidance: "Enter a local Whisper model path in Settings, save it, then use Test path.",
        compatibilityNote: "Readability does not prove model compatibility.",
        lastPathTest: null,
        lastSuccessfulTranscription: null,
      },
      ollama: {
        state: "ConfiguredNotChecked",
        baseUrl: "http://127.0.0.1:11434",
        model: "qwen3.6:27b",
        availability: "UnknownUntilTest",
        message: "Ollama is configured for a local loopback URL and model.",
        setupGuidance:
          "Start Ollama manually, install the selected local model if needed, then run Test Ollama. Availability is unknown until Test Ollama runs.",
        lastConnectionTest: null,
      },
    },
    calendarContext: {
      source: "AppleCalendar",
      permissionState: "Unavailable",
      availabilityState: "Unavailable",
      message: "Apple Calendar context is unavailable until desktop commands load.",
      setupGuidance:
        "Calendar context is read-only and recordings never start from calendar events automatically.",
      upcomingEvents: [],
      autoStartEnabled: false,
    },
    settings: {
      whisperModelPath: "",
      ollamaBaseUrl: "http://127.0.0.1:11434",
      ollamaModel: "qwen3.6:27b",
      exportDirectory: null,
      rawAudioRetentionPolicy: "Retain",
    },
    capture: {
      microphone: "MicrophoneUnavailable",
      systemAudio: "SystemAudioUnavailable",
    },
    transcription: null,
    transcriptionJob: null,
    exportCommand: {
      state: "idle",
    },
    deleteCommand: {
      state: "idle",
    },
    analysisCommand: null,
    summaryJob: null,
  };
}

function meeting(
  input: Omit<MeetingView, "status" | "privacy" | "exportState" | "deleteState" | "calendarAttachment"> &
    Partial<Pick<MeetingView, "calendarAttachment">>,
): MeetingView {
  return {
    ...input,
    status: "Complete",
    privacy: {
      storageLabel: "Private storage",
      storagePath: `meetings/${input.id}/audio`,
      rawAudioRetention: "Retain",
      localOnly: !input.analysis?.networkUsed,
    },
    exportState: {
      state: "idle",
    },
    deleteState: {
      state: "idle",
    },
    calendarAttachment: input.calendarAttachment ?? null,
  };
}

function segment(
  id: string,
  startMs: number,
  endMs: number,
  text: string,
  sourceChannel: string,
): TranscriptSegment {
  return {
    id,
    startMs,
    endMs,
    text,
    originalText: null,
    sourceChannel,
    modelRunId: "run-1",
    transcriptVersionId: "version-1",
  };
}

type ContractPath = readonly string[];
type ContractRecord = Record<string, unknown>;

const REQUIRED_DESKTOP_SNAPSHOT_PATHS: readonly ContractPath[] = [
  ["loading"],
  ["commandSurface", "ready"],
  ["commandSurface", "detail"],
  ["meetings"],
  ["selectedMeetingId"],
  ["recording", "meeting_id"],
  ["recording", "recording_id"],
  ["recording", "state"],
  ["recording", "permission_state"],
  ["recording", "storage_location", "app_private_path"],
  ["recording", "raw_audio_retention"],
  ["recording", "recoverable"],
  ["recording", "recovery_action"],
  ["model", "kind"],
  ["model", "configuredPath"],
  ["setupGuidance", "whisper", "state"],
  ["setupGuidance", "whisper", "configuredPath"],
  ["setupGuidance", "whisper", "message"],
  ["setupGuidance", "whisper", "setupGuidance"],
  ["setupGuidance", "whisper", "compatibilityNote"],
  ["setupGuidance", "whisper", "lastPathTest"],
  ["setupGuidance", "whisper", "lastSuccessfulTranscription"],
  ["setupGuidance", "ollama", "state"],
  ["setupGuidance", "ollama", "baseUrl"],
  ["setupGuidance", "ollama", "model"],
  ["setupGuidance", "ollama", "availability"],
  ["setupGuidance", "ollama", "message"],
  ["setupGuidance", "ollama", "setupGuidance"],
  ["setupGuidance", "ollama", "lastConnectionTest"],
  ["calendarContext", "source"],
  ["calendarContext", "permissionState"],
  ["calendarContext", "availabilityState"],
  ["calendarContext", "message"],
  ["calendarContext", "setupGuidance"],
  ["calendarContext", "upcomingEvents"],
  ["calendarContext", "autoStartEnabled"],
  ["settings", "whisperModelPath"],
  ["settings", "ollamaBaseUrl"],
  ["settings", "ollamaModel"],
  ["settings", "exportDirectory"],
  ["settings", "rawAudioRetentionPolicy"],
  ["capture", "microphone"],
  ["capture", "systemAudio"],
  ["transcription"],
  ["transcriptionJob"],
  ["exportCommand", "state"],
  ["deleteCommand", "state"],
  ["analysisCommand"],
  ["summaryJob"],
];

const REQUIRED_MEETING_PATHS: readonly ContractPath[] = [
  ["id"],
  ["title"],
  ["startedAt"],
  ["duration"],
  ["status"],
  ["transcriptState"],
  ["transcriptText"],
  ["segments"],
  ["privacy", "storageLabel"],
  ["privacy", "storagePath"],
  ["privacy", "rawAudioRetention"],
  ["privacy", "localOnly"],
  ["exportState", "state"],
  ["deleteState", "state"],
  ["calendarAttachment"],
  ["analysis"],
];

const REQUIRED_SEGMENT_PATHS: readonly ContractPath[] = [
  ["id"],
  ["startMs"],
  ["endMs"],
  ["text"],
  ["originalText"],
  ["sourceChannel"],
  ["modelRunId"],
  ["transcriptVersionId"],
];

const DESKTOP_SNAPSHOT_COMMANDS = new Set([
  "desktop_snapshot",
  "correct_transcript_segment",
  "delete_meeting",
  "export_meeting",
  "export_meeting_json",
  "generate_summary",
  "cancel_summary",
  "attach_calendar_event_context",
  "rename_meeting",
  "request_apple_calendar_access",
  "save_analysis_settings",
  "save_raw_audio_retention_policy",
  "save_whisper_model_path",
  "import_audio_file",
  "start_microphone_recording",
  "stop_microphone_recording",
  "transcribe_meeting",
  "cancel_transcription",
]);

function requireContractPath(value: unknown, path: ContractPath, rootLabel: string): unknown {
  let current = value;
  let currentPath = rootLabel;

  for (const field of path) {
    const record = requireContractRecord(current, currentPath);
    if (!Object.prototype.hasOwnProperty.call(record, field)) {
      throw new Error(`desktop_snapshot contract drift: missing ${currentPath}.${field}`);
    }
    current = record[field];
    currentPath = `${currentPath}.${field}`;
  }

  return current;
}

function requireContractRecord(value: unknown, pathLabel: string): ContractRecord {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(`desktop_snapshot contract drift: expected ${pathLabel} to be an object`);
  }
  return value as ContractRecord;
}

function requireContractArray(value: unknown, pathLabel: string): unknown[] {
  if (!Array.isArray(value)) {
    throw new Error(`desktop_snapshot contract drift: expected ${pathLabel} to be an array`);
  }
  return value;
}

function requireString(value: unknown, pathLabel: string): string {
  if (typeof value !== "string") {
    throw new Error(`desktop_snapshot contract drift: expected ${pathLabel} to be a string`);
  }
  return value;
}

function requireNonEmptyString(value: unknown, pathLabel: string): string {
  const string = requireString(value, pathLabel);
  if (!string.trim()) {
    throw new Error(`desktop_snapshot contract drift: expected ${pathLabel} to be a non-empty string`);
  }
  return string;
}

function requireNullableString(value: unknown, pathLabel: string): string | null {
  if (value === null) {
    return value;
  }
  return requireString(value, pathLabel);
}

function requireNullableStringArray(value: unknown, pathLabel: string): string[] | null {
  if (value === null) {
    return value;
  }
  return requireContractArray(value, pathLabel).map((item, index) =>
    requireString(item, `${pathLabel}[${index}]`),
  );
}

function requireNullableNumber(value: unknown, pathLabel: string): number | null {
  if (value === null) {
    return value;
  }
  return requireNumber(value, pathLabel);
}

function requireNumber(value: unknown, pathLabel: string): number {
  if (typeof value !== "number" || !Number.isFinite(value)) {
    throw new Error(`desktop_snapshot contract drift: expected ${pathLabel} to be a finite number`);
  }
  return value;
}

function requireBoolean(value: unknown, pathLabel: string): boolean {
  if (typeof value !== "boolean") {
    throw new Error(`desktop_snapshot contract drift: expected ${pathLabel} to be a boolean`);
  }
  return value;
}

function requireEnum<const T extends string>(
  value: unknown,
  allowed: readonly T[],
  pathLabel: string,
): T {
  if (typeof value !== "string" || !allowed.includes(value as T)) {
    throw new Error(`desktop_snapshot contract drift: expected ${pathLabel} to be one of ${allowed.join(", ")}`);
  }
  return value as T;
}

function validateFirstRunSetupGuidance(value: unknown, pathLabel: string, configuredModelPath: string): void {
  const guidance = requireContractRecord(value, pathLabel);
  const whisper = requireContractRecord(guidance.whisper, `${pathLabel}.whisper`);
  requireEnum(
    whisper.state,
    ["MissingPath", "UnreadablePath", "ReadablePath"],
    `${pathLabel}.whisper.state`,
  );
  const whisperConfiguredPath = requireString(whisper.configuredPath, `${pathLabel}.whisper.configuredPath`);
  if (whisperConfiguredPath !== configuredModelPath) {
    throw new Error(
      `desktop_snapshot contract drift: expected ${pathLabel}.whisper.configuredPath to match desktop_snapshot.model.configuredPath`,
    );
  }
  requireString(whisper.message, `${pathLabel}.whisper.message`);
  requireString(whisper.setupGuidance, `${pathLabel}.whisper.setupGuidance`);
  requireString(whisper.compatibilityNote, `${pathLabel}.whisper.compatibilityNote`);
  validateWhisperPathTestEvidence(whisper.lastPathTest, `${pathLabel}.whisper.lastPathTest`);
  validateWhisperTranscriptionCompatibilityEvidence(
    whisper.lastSuccessfulTranscription,
    `${pathLabel}.whisper.lastSuccessfulTranscription`,
    configuredModelPath,
  );

  const ollama = requireContractRecord(guidance.ollama, `${pathLabel}.ollama`);
  requireEnum(
    ollama.state,
    ["ConfiguredNotChecked", "InvalidLocalConfiguration"],
    `${pathLabel}.ollama.state`,
  );
  const baseUrl = requireString(ollama.baseUrl, `${pathLabel}.ollama.baseUrl`);
  const model = requireString(ollama.model, `${pathLabel}.ollama.model`);
  const availability = requireEnum(
    ollama.availability,
    ["UnknownUntilTest", "AvailableAtLastTest", "MissingModelAtLastTest", "UnavailableAtLastTest"],
    `${pathLabel}.ollama.availability`,
  );
  requireString(ollama.message, `${pathLabel}.ollama.message`);
  requireString(ollama.setupGuidance, `${pathLabel}.ollama.setupGuidance`);
  validateOllamaConnectionTestEvidence(ollama.lastConnectionTest, `${pathLabel}.ollama.lastConnectionTest`);
  validateOllamaSetupAvailabilityEvidence(
    availability,
    ollama.lastConnectionTest,
    `${pathLabel}.ollama.lastConnectionTest`,
    baseUrl,
    model,
  );
}

function validateWhisperPathTestEvidence(value: unknown, pathLabel: string): void {
  if (value === null) {
    return;
  }
  const evidence = requireContractRecord(value, pathLabel);
  requireString(evidence.testedPath, `${pathLabel}.testedPath`);
  requireNonNegativeInteger(evidence.testedAtMs, `${pathLabel}.testedAtMs`);
  requireEnum(evidence.state, ["Valid", "Invalid"], `${pathLabel}.state`);
  const fileSizeBytes = requireNullableNumber(evidence.fileSizeBytes, `${pathLabel}.fileSizeBytes`);
  if (fileSizeBytes !== null) {
    requireNonNegativeInteger(fileSizeBytes, `${pathLabel}.fileSizeBytes`);
  }
  const sha256 = requireNullableString(evidence.sha256, `${pathLabel}.sha256`);
  if (sha256 !== null && !/^[a-f0-9]{64}$/.test(sha256)) {
    throw new Error(`desktop_snapshot contract drift: expected ${pathLabel}.sha256 to be a SHA-256 hex string`);
  }
  requireNullableString(evidence.failureDetail, `${pathLabel}.failureDetail`);
}

function validateWhisperTranscriptionCompatibilityEvidence(
  value: unknown,
  pathLabel: string,
  configuredModelPath: string,
): void {
  if (value === null) {
    return;
  }
  const evidence = requireContractRecord(value, pathLabel);
  const modelPath = requireNonEmptyString(evidence.modelPath, `${pathLabel}.modelPath`);
  if (modelPath !== configuredModelPath) {
    throw new Error(
      `desktop_snapshot contract drift: expected ${pathLabel}.modelPath to match desktop_snapshot.model.configuredPath`,
    );
  }
  requireNonNegativeInteger(evidence.usedAtMs, `${pathLabel}.usedAtMs`);
  requireNonEmptyString(evidence.provider, `${pathLabel}.provider`);
  requireNonEmptyString(evidence.modelName, `${pathLabel}.modelName`);
  requireNonEmptyString(evidence.meetingId, `${pathLabel}.meetingId`);
  requireNonEmptyString(evidence.modelRunId, `${pathLabel}.modelRunId`);
  requireNonEmptyString(evidence.transcriptVersionId, `${pathLabel}.transcriptVersionId`);
  requirePositiveInteger(evidence.segmentCount, `${pathLabel}.segmentCount`);
  requirePositiveInteger(evidence.fileSizeBytes, `${pathLabel}.fileSizeBytes`);
  requireNonNegativeInteger(evidence.modifiedAtMs, `${pathLabel}.modifiedAtMs`);
}

function validateWhisperModelReadinessEvidence(
  modelKind: ModelStatus["kind"],
  configuredModelPath: string,
  setupGuidanceValue: unknown,
): void {
  const setupGuidance = requireContractRecord(setupGuidanceValue, "desktop_snapshot.setupGuidance");
  const whisper = requireContractRecord(setupGuidance.whisper, "desktop_snapshot.setupGuidance.whisper");
  const evidencePath = "desktop_snapshot.setupGuidance.whisper.lastPathTest";
  const evidenceValue = whisper.lastPathTest;

  if (modelKind !== "ready" && modelKind !== "untested") {
    return;
  }

  if (modelKind === "ready" && evidenceValue === null) {
    throw new Error(`desktop_snapshot contract drift: expected ${evidencePath} for ready Whisper model`);
  }

  if (evidenceValue === null) {
    return;
  }

  const evidence = requireContractRecord(evidenceValue, evidencePath);
  const testedPath = requireString(evidence.testedPath, `${evidencePath}.testedPath`);
  const evidenceState = requireEnum(evidence.state, ["Valid", "Invalid"], `${evidencePath}.state`);
  const fileSizeBytes = requireNullableNumber(evidence.fileSizeBytes, `${evidencePath}.fileSizeBytes`);

  if (modelKind === "ready") {
    if (evidenceState !== "Valid") {
      throw new Error(`desktop_snapshot contract drift: expected ${evidencePath}.state to be Valid for ready Whisper model`);
    }
    if (testedPath !== configuredModelPath) {
      throw new Error(
        `desktop_snapshot contract drift: expected ${evidencePath}.testedPath to match desktop_snapshot.model.configuredPath`,
      );
    }
    if (fileSizeBytes === null) {
      throw new Error(
        `desktop_snapshot contract drift: expected ${evidencePath}.fileSizeBytes for ready Whisper model`,
      );
    }
    return;
  }

  if (evidenceState === "Valid" && testedPath === configuredModelPath && fileSizeBytes !== null) {
    throw new Error(
      `desktop_snapshot contract drift: expected desktop_snapshot.model.kind to be ready when ${evidencePath} matches the configured Whisper path`,
    );
  }
}

function validateOllamaConnectionTestEvidence(value: unknown, pathLabel: string): void {
  if (value === null) {
    return;
  }
  const evidence = requireContractRecord(value, pathLabel);
  requireString(evidence.baseUrl, `${pathLabel}.baseUrl`);
  requireString(evidence.requestedModel, `${pathLabel}.requestedModel`);
  requireNonNegativeInteger(evidence.testedAtMs, `${pathLabel}.testedAtMs`);
  requireEnum(evidence.state, ["Available", "Unavailable"], `${pathLabel}.state`);
  requireNullableString(evidence.selectedLocalModelTag, `${pathLabel}.selectedLocalModelTag`);
  requireNullableStringArray(evidence.installedLocalModels, `${pathLabel}.installedLocalModels`);
  requireNullableString(evidence.pullCommand, `${pathLabel}.pullCommand`);
  requireNullableString(evidence.failureDetail, `${pathLabel}.failureDetail`);
}

function validateOllamaSetupAvailabilityEvidence(
  availability: OllamaAvailabilityState,
  value: unknown,
  pathLabel: string,
  expectedBaseUrl: string,
  expectedModel: string,
): void {
  if (availability === "UnknownUntilTest") {
    if (value !== null) {
      throw new Error(`desktop_snapshot contract drift: expected ${pathLabel} to be null for ${availability}`);
    }
    return;
  }
  if (value === null) {
    throw new Error(`desktop_snapshot contract drift: expected ${pathLabel} for ${availability}`);
  }
  const evidence = requireContractRecord(value, pathLabel);
  if (evidence.baseUrl !== expectedBaseUrl) {
    throw new Error(`desktop_snapshot contract drift: expected ${pathLabel}.baseUrl to match setup guidance`);
  }
  if (evidence.requestedModel !== expectedModel) {
    throw new Error(`desktop_snapshot contract drift: expected ${pathLabel}.requestedModel to match setup guidance`);
  }
  if (availability === "AvailableAtLastTest") {
    if (evidence.state !== "Available") {
      throw new Error(`desktop_snapshot contract drift: expected ${pathLabel}.state to be Available for ${availability}`);
    }
    return;
  }
  if (evidence.state !== "Unavailable") {
    throw new Error(`desktop_snapshot contract drift: expected ${pathLabel}.state to be Unavailable for ${availability}`);
  }
  const pullCommand = evidence.pullCommand;
  if (availability === "MissingModelAtLastTest") {
    if (typeof pullCommand !== "string" || pullCommand.trim() === "") {
      throw new Error(
        `desktop_snapshot contract drift: expected ${pathLabel}.pullCommand to be a non-empty pull command for ${availability}`,
      );
    }
    return;
  }
  if (typeof pullCommand === "string" && pullCommand.trim() !== "") {
    throw new Error(`desktop_snapshot contract drift: expected ${pathLabel}.pullCommand to be empty for ${availability}`);
  }
}

function validateCalendarContext(value: unknown, pathLabel: string): void {
  const context = requireContractRecord(value, pathLabel);
  requireEnum(context.source, ["AppleCalendar"], `${pathLabel}.source`);
  requireEnum(context.permissionState, ["NotRequested", "Granted", "Denied", "Unavailable"], `${pathLabel}.permissionState`);
  requireEnum(context.availabilityState, ["Unavailable", "PermissionRequired", "Ready"], `${pathLabel}.availabilityState`);
  requireString(context.message, `${pathLabel}.message`);
  requireString(context.setupGuidance, `${pathLabel}.setupGuidance`);
  requireFalse(context.autoStartEnabled, `${pathLabel}.autoStartEnabled`);
  requireContractArray(context.upcomingEvents, `${pathLabel}.upcomingEvents`).forEach((event, index) => {
    validateCalendarContextEvent(event, `${pathLabel}.upcomingEvents[${index}]`);
  });
}

function validateCalendarContextEvent(value: unknown, pathLabel: string): void {
  const event = requireContractRecord(value, pathLabel);
  requireString(event.id, `${pathLabel}.id`);
  requireString(event.title, `${pathLabel}.title`);
  requireString(event.calendarTitle, `${pathLabel}.calendarTitle`);
  requireNonNegativeInteger(event.startsAtMs, `${pathLabel}.startsAtMs`);
  requireNonNegativeInteger(event.endsAtMs, `${pathLabel}.endsAtMs`);
  requireBoolean(event.isAllDay, `${pathLabel}.isAllDay`);
  requireBoolean(event.isRecurring, `${pathLabel}.isRecurring`);
  requireEnum(event.privacy, ["Unknown", "Public", "Private"], `${pathLabel}.privacy`);
  requireEnum(event.overlapState, ["None", "Ambiguous", "Overlapping"], `${pathLabel}.overlapState`);
  requireBoolean(event.attachable, `${pathLabel}.attachable`);
  requireString(event.safetyNote, `${pathLabel}.safetyNote`);
}

function validateMeetingCalendarAttachment(value: unknown, pathLabel: string): void {
  if (value === null) {
    return;
  }
  const attachment = requireContractRecord(value, pathLabel);
  requireEnum(attachment.source, ["AppleCalendar"], `${pathLabel}.source`);
  requireString(attachment.eventId, `${pathLabel}.eventId`);
  requireString(attachment.eventTitle, `${pathLabel}.eventTitle`);
  requireString(attachment.calendarTitle, `${pathLabel}.calendarTitle`);
  requireNonNegativeInteger(attachment.startsAtMs, `${pathLabel}.startsAtMs`);
  requireNonNegativeInteger(attachment.endsAtMs, `${pathLabel}.endsAtMs`);
  requireEnum(attachment.privacy, ["Unknown", "Public", "Private"], `${pathLabel}.privacy`);
  requireBoolean(attachment.privacyConfirmed, `${pathLabel}.privacyConfirmed`);
  requireNonNegativeInteger(attachment.attachedAtMs, `${pathLabel}.attachedAtMs`);
}

function requireNonNegativeInteger(value: unknown, pathLabel: string): number {
  const number = requireNumber(value, pathLabel);
  if (!Number.isInteger(number) || number < 0) {
    throw new Error(`desktop_snapshot contract drift: expected ${pathLabel} to be a non-negative integer`);
  }
  return number;
}

function requirePositiveInteger(value: unknown, pathLabel: string): number {
  const number = requireNumber(value, pathLabel);
  if (!Number.isInteger(number) || number <= 0) {
    throw new Error(`desktop_snapshot contract drift: expected ${pathLabel} to be a positive integer`);
  }
  return number;
}

function requireFalse(value: unknown, pathLabel: string): false {
  if (value !== false) {
    throw new Error(`desktop_snapshot contract drift: expected ${pathLabel} to be false`);
  }
  return value;
}

function validateExportCommandState(value: unknown, pathLabel: string): void {
  const state = requireContractRecord(value, pathLabel);
  const exportState = requireEnum(state.state, ["idle", "exporting", "exported", "failed"], `${pathLabel}.state`);
  if (Object.prototype.hasOwnProperty.call(state, "meetingId")) {
    requireNullableString(state.meetingId, `${pathLabel}.meetingId`);
  }
  if (exportState !== "idle") {
    requireEnum(state.format, ["json", "markdown", "srt"], `${pathLabel}.format`);
  } else if (Object.prototype.hasOwnProperty.call(state, "format")) {
    if (state.format !== null) {
      requireEnum(state.format, ["json", "markdown", "srt"], `${pathLabel}.format`);
    }
  }
  if (Object.prototype.hasOwnProperty.call(state, "path")) {
    requireNullableString(state.path, `${pathLabel}.path`);
  }
  if (Object.prototype.hasOwnProperty.call(state, "message")) {
    requireNullableString(state.message, `${pathLabel}.message`);
  }
}

function validateDeleteCommandState(value: unknown, pathLabel: string): void {
  const state = requireContractRecord(value, pathLabel);
  requireEnum(state.state, ["idle", "deleting", "deleted", "failed"], `${pathLabel}.state`);
  if (Object.prototype.hasOwnProperty.call(state, "meetingId")) {
    requireNullableString(state.meetingId, `${pathLabel}.meetingId`);
  }
  if (Object.prototype.hasOwnProperty.call(state, "message")) {
    requireNullableString(state.message, `${pathLabel}.message`);
  }
  for (const field of ["deletedPrivateArtifacts", "skippedPrivateArtifacts", "remainingExports"]) {
    if (Object.prototype.hasOwnProperty.call(state, field)) {
      requireContractArray(state[field], `${pathLabel}.${field}`).forEach((item, index) => {
        requireString(item, `${pathLabel}.${field}[${index}]`);
      });
    }
  }
}

function validateAnalysisDisclosureState(value: unknown, pathLabel: string): void {
  if (value === null) {
    return;
  }
  const state = requireContractRecord(value, pathLabel);
  requireString(state.provider, `${pathLabel}.provider`);
  requireString(state.modelName, `${pathLabel}.modelName`);
  requireBoolean(state.networkUsed, `${pathLabel}.networkUsed`);
  requireBoolean(state.disclosureRequired, `${pathLabel}.disclosureRequired`);
  requireBoolean(state.disclosureConfirmed, `${pathLabel}.disclosureConfirmed`);
  requireString(state.summary, `${pathLabel}.summary`);
  requireNumber(state.createdAtMs, `${pathLabel}.createdAtMs`);
  requireString(state.promptTemplateVersion, `${pathLabel}.promptTemplateVersion`);
}

function validateCommandFailureView(value: unknown, pathLabel: string): void {
  if (value === null) {
    return;
  }
  const failure = requireContractRecord(value, pathLabel);
  requireString(failure.code, `${pathLabel}.code`);
  requireString(failure.message, `${pathLabel}.message`);
  requireString(failure.setupGuidance, `${pathLabel}.setupGuidance`);
}

function validateAnalysisResultView(value: unknown, pathLabel: string): void {
  if (value === null) {
    return;
  }
  const analysis = requireContractRecord(value, pathLabel);
  requireString(analysis.provider, `${pathLabel}.provider`);
  requireString(analysis.modelName, `${pathLabel}.modelName`);
  requireBoolean(analysis.networkUsed, `${pathLabel}.networkUsed`);
  requireString(analysis.summary, `${pathLabel}.summary`);
}

function validateTranscriptionCommandView(value: unknown, pathLabel: string): void {
  if (value === null) {
    return;
  }
  const command = requireContractRecord(value, pathLabel);
  requireString(command.meetingId, `${pathLabel}.meetingId`);
  requireEnum(command.state, ["Complete", "Failed"], `${pathLabel}.state`);
  validateCommandFailureView(command.failure, `${pathLabel}.failure`);
}

function validateCommandJobView(value: unknown, pathLabel: string): void {
  if (value === null) {
    return;
  }
  const job = requireContractRecord(value, pathLabel);
  requireString(job.id, `${pathLabel}.id`);
  requireEnum(job.kind, ["Transcription", "Summary"], `${pathLabel}.kind`);
  requireString(job.meetingId, `${pathLabel}.meetingId`);
  requireEnum(job.state, ["Running", "CancelRequested", "Complete", "Failed", "Recovery", "Retry", "Canceled"], `${pathLabel}.state`);
  requireBoolean(job.cancelRequested, `${pathLabel}.cancelRequested`);
  requireNumber(job.startedAtMs, `${pathLabel}.startedAtMs`);
  if (Object.prototype.hasOwnProperty.call(job, "lastError")) {
    requireNullableString(job.lastError, `${pathLabel}.lastError`);
  }
}

function validateAnalysisCommandView(value: unknown, pathLabel: string): void {
  if (value === null) {
    return;
  }
  const command = requireContractRecord(value, pathLabel);
  requireString(command.meetingId, `${pathLabel}.meetingId`);
  requireEnum(command.state, ["Complete", "Failed"], `${pathLabel}.state`);
  validateAnalysisResultView(command.analysis, `${pathLabel}.analysis`);
  validateCommandFailureView(command.failure, `${pathLabel}.failure`);
}

function assertWhisperModelPathTestContract(value: unknown): asserts value is WhisperModelPathTestResult {
  const result = requireContractRecord(value, "test_whisper_model_path");
  const state = requireEnum(result.state, ["Valid", "Invalid"], "test_whisper_model_path.state");
  requireString(result.message, "test_whisper_model_path.message");
  requireString(result.setupGuidance, "test_whisper_model_path.setupGuidance");

  if (state === "Valid") {
    const fileSizeBytes = requireNumber(result.fileSizeBytes, "test_whisper_model_path.fileSizeBytes");
    if (!Number.isInteger(fileSizeBytes) || fileSizeBytes < 0) {
      throw new Error("desktop_snapshot contract drift: expected test_whisper_model_path.fileSizeBytes to be a non-negative integer");
    }
    const sha256 = requireString(result.sha256, "test_whisper_model_path.sha256");
    if (!/^[a-f0-9]{64}$/.test(sha256)) {
      throw new Error("desktop_snapshot contract drift: expected test_whisper_model_path.sha256 to be a SHA-256 hex string");
    }
  }
}

function assertOllamaConnectionTestContract(value: unknown): asserts value is OllamaConnectionTestResult {
  const result = requireContractRecord(value, "test_ollama_connection");
  requireEnum(result.state, ["Available", "Unavailable"], "test_ollama_connection.state");
  requireString(result.message, "test_ollama_connection.message");
  requireString(result.setupGuidance, "test_ollama_connection.setupGuidance");
  requireNullableString(result.selectedLocalModelTag, "test_ollama_connection.selectedLocalModelTag");
  requireNullableStringArray(result.installedLocalModels, "test_ollama_connection.installedLocalModels");
  requireNullableString(result.pullCommand, "test_ollama_connection.pullCommand");
}

function assertMeetingSearchResultsContract(value: unknown): asserts value is MeetingSearchResult[] {
  requireContractArray(value, "search_meetings").forEach((result, index) => {
    const pathLabel = `search_meetings[${index}]`;
    const record = requireContractRecord(result, pathLabel);
    requireNonEmptyString(record.meeting_id, `${pathLabel}.meeting_id`);
    requireString(record.title, `${pathLabel}.title`);
  });
}

function retentionDetail(policy: RawAudioRetentionPolicy): string {
  if (policy === "DeleteAfterTranscription") {
    return "Raw audio will be deleted after transcription.";
  }
  if (policy === "NeverSave") {
    return "Raw audio was not saved for this meeting.";
  }
  return "Raw audio retained in private app storage.";
}

export function getDesktopCommandFetcher(): CommandFetcher | undefined {
  if (!isTauriRuntime()) {
    return undefined;
  }
  return async <T>(command: string, args?: Record<string, unknown>) => {
    const { invoke } = await import("@tauri-apps/api/core");
    const result = await invoke<unknown>(command, args);
    if (DESKTOP_SNAPSHOT_COMMANDS.has(command)) {
      assertDesktopSnapshotContract(result);
    }
    if (command === "test_whisper_model_path") {
      assertWhisperModelPathTestContract(result);
    }
    if (command === "test_ollama_connection") {
      assertOllamaConnectionTestContract(result);
    }
    if (command === "search_meetings") {
      assertMeetingSearchResultsContract(result);
    }
    return result as T;
  };
}

export function createDesktopCommandFacade(fetchCommand: CommandFetcher): DesktopCommandFacade {
  async function snapshotCommand(command: string, args?: Record<string, unknown>): Promise<DesktopSnapshot> {
    const result = await fetchCommand<unknown>(command, args);
    assertDesktopSnapshotContract(result);
    return result;
  }

  return {
    desktopSnapshot: () => snapshotCommand("desktop_snapshot"),
    searchMeetings: async ({ query }) => {
      const result = await fetchCommand<unknown>("search_meetings", { query });
      assertMeetingSearchResultsContract(result);
      return result;
    },
    startRecording: (args) =>
      snapshotCommand("start_microphone_recording", args?.title ? { title: args.title } : undefined),
    importAudioFile: ({ sourcePath, title }) =>
      snapshotCommand("import_audio_file", title ? { sourcePath, title } : { sourcePath }),
    stopRecording: () => snapshotCommand("stop_microphone_recording"),
    transcribeMeeting: ({ meetingId }) => snapshotCommand("transcribe_meeting", { meetingId }),
    correctTranscriptSegment: ({ meetingId, segmentId, correctedText, editedAtMs }) =>
      snapshotCommand("correct_transcript_segment", { meetingId, segmentId, correctedText, editedAtMs }),
    cancelTranscription: ({ jobId }) => snapshotCommand("cancel_transcription", { jobId }),
    renameMeeting: ({ meetingId, title }) => snapshotCommand("rename_meeting", { meetingId, title }),
    exportMeeting: ({ meetingId, format }) => snapshotCommand("export_meeting", { meetingId, format }),
    exportMeetingJson: ({ meetingId }) => snapshotCommand("export_meeting_json", { meetingId }),
    deleteMeeting: ({ meetingId }) => snapshotCommand("delete_meeting", { meetingId }),
    generateSummary: ({ meetingId }) => snapshotCommand("generate_summary", { meetingId }),
    cancelSummary: ({ jobId }) => snapshotCommand("cancel_summary", { jobId }),
    saveWhisperModelPath: ({ whisperModelPath }) =>
      snapshotCommand("save_whisper_model_path", { whisperModelPath }),
    saveAnalysisSettings: ({ ollamaBaseUrl, ollamaModel }) =>
      snapshotCommand("save_analysis_settings", { ollamaBaseUrl, ollamaModel }),
    saveRawAudioRetentionPolicy: ({ rawAudioRetentionPolicy }) =>
      snapshotCommand("save_raw_audio_retention_policy", { rawAudioRetentionPolicy }),
    requestAppleCalendarAccess: () => snapshotCommand("request_apple_calendar_access"),
    attachCalendarEventContext: ({ meetingId, eventId, privacyConfirmed }) =>
      snapshotCommand("attach_calendar_event_context", { meetingId, eventId, privacyConfirmed }),
    testWhisperModelPath: async ({ path }) => {
      const result = await fetchCommand<unknown>("test_whisper_model_path", { path });
      assertWhisperModelPathTestContract(result);
      return result;
    },
    testOllamaConnection: async ({ baseUrl, model }) => {
      const result = await fetchCommand<unknown>("test_ollama_connection", { baseUrl, model });
      assertOllamaConnectionTestContract(result);
      return result;
    },
  };
}

export function getDesktopCommandFacade(): DesktopCommandFacade | undefined {
  const fetchCommand = getDesktopCommandFetcher();
  return fetchCommand ? createDesktopCommandFacade(fetchCommand) : undefined;
}
