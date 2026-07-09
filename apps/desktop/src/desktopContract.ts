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
const LOCAL_OLLAMA_SETUP_CANDIDATES = [
  { id: "ollama-qwen3-6-27b", modelTag: "qwen3.6:27b" },
  { id: "ollama-gemma4-31b", modelTag: "gemma4:31b" },
] as const;

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
  kind: "ready" | "missing" | "untested" | "unsupported" | "transcribing";
  configuredPath: string;
}

export type WhisperSetupState = "MissingPath" | "UnreadablePath" | "UnsupportedFile" | "ReadablePath";
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

export interface ModelSetupOptions {
  whisper: WhisperModelSetupOptions;
  ollama: OllamaModelSetupOptions;
}

export interface WhisperModelSetupOptions {
  mode: "ManualFile";
  title: string;
  detail: string;
  chooseLabel: string;
  saveLabel: string;
  testLabel: string;
  downloadsManaged: false;
  acceptedExtensions: string[];
}

export interface OllamaModelSetupOptions {
  mode: "ManualOllama";
  title: string;
  detail: string;
  automaticPulls: false;
  candidates: OllamaModelSetupCandidate[];
}

export interface OllamaModelSetupCandidate {
  id: string;
  displayName: string;
  modelTag: string;
  pullCommand: string;
  defaultCandidate: boolean;
  setupNotes: string;
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
  modelSetupOptions: ModelSetupOptions;
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
    ["ready", "missing", "untested", "unsupported", "transcribing"],
    "desktop_snapshot.model.kind",
  );
  const configuredModelPath = requireString(model.configuredPath, "desktop_snapshot.model.configuredPath");

  validateFirstRunSetupGuidance(root.setupGuidance, "desktop_snapshot.setupGuidance", configuredModelPath, modelKind);
  validateWhisperModelReadinessEvidence(modelKind, configuredModelPath, root.setupGuidance);
  validateModelSetupOptions(root.modelSetupOptions, "desktop_snapshot.modelSetupOptions");
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
  ["modelSetupOptions", "whisper", "mode"],
  ["modelSetupOptions", "whisper", "title"],
  ["modelSetupOptions", "whisper", "detail"],
  ["modelSetupOptions", "whisper", "chooseLabel"],
  ["modelSetupOptions", "whisper", "saveLabel"],
  ["modelSetupOptions", "whisper", "testLabel"],
  ["modelSetupOptions", "whisper", "downloadsManaged"],
  ["modelSetupOptions", "whisper", "acceptedExtensions"],
  ["modelSetupOptions", "ollama", "mode"],
  ["modelSetupOptions", "ollama", "title"],
  ["modelSetupOptions", "ollama", "detail"],
  ["modelSetupOptions", "ollama", "automaticPulls"],
  ["modelSetupOptions", "ollama", "candidates"],
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

function validateFirstRunSetupGuidance(
  value: unknown,
  pathLabel: string,
  configuredModelPath: string,
  modelKind: ModelStatus["kind"],
): void {
  const guidance = requireContractRecord(value, pathLabel);
  const whisper = requireContractRecord(guidance.whisper, `${pathLabel}.whisper`);
  const whisperState = requireEnum(
    whisper.state,
    ["MissingPath", "UnreadablePath", "UnsupportedFile", "ReadablePath"],
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
  if (modelKind === "unsupported" && whisperState === "UnsupportedFile") {
    if (whisper.lastPathTest !== null) {
      throw new Error(
        `desktop_snapshot contract drift: expected ${pathLabel}.whisper.lastPathTest to be null for unsupported Whisper model files`,
      );
    }
    if (whisper.lastSuccessfulTranscription !== null) {
      throw new Error(
        `desktop_snapshot contract drift: expected ${pathLabel}.whisper.lastSuccessfulTranscription to be null for unsupported Whisper model files`,
      );
    }
  }

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

function validateModelSetupOptions(value: unknown, pathLabel: string): void {
  const options = requireContractRecord(value, pathLabel);

  const whisper = requireContractRecord(options.whisper, `${pathLabel}.whisper`);
  requireEnum(whisper.mode, ["ManualFile"], `${pathLabel}.whisper.mode`);
  requireNonEmptyString(whisper.title, `${pathLabel}.whisper.title`);
  requireNonEmptyString(whisper.detail, `${pathLabel}.whisper.detail`);
  requireNonEmptyString(whisper.chooseLabel, `${pathLabel}.whisper.chooseLabel`);
  requireNonEmptyString(whisper.saveLabel, `${pathLabel}.whisper.saveLabel`);
  requireNonEmptyString(whisper.testLabel, `${pathLabel}.whisper.testLabel`);
  requireFalse(whisper.downloadsManaged, `${pathLabel}.whisper.downloadsManaged`);
  const acceptedExtensions = requireContractArray(
    whisper.acceptedExtensions,
    `${pathLabel}.whisper.acceptedExtensions`,
  ).map((extension, index) =>
    requireNonEmptyString(extension, `${pathLabel}.whisper.acceptedExtensions[${index}]`),
  );
  const supportedWhisperExtensions = ["bin", "gguf"];
  if (
    acceptedExtensions.length !== supportedWhisperExtensions.length ||
    acceptedExtensions.some((extension, index) => extension !== supportedWhisperExtensions[index])
  ) {
    throw new Error(
      `desktop_snapshot contract drift: expected ${pathLabel}.whisper.acceptedExtensions to equal bin, gguf`,
    );
  }

  const ollama = requireContractRecord(options.ollama, `${pathLabel}.ollama`);
  requireEnum(ollama.mode, ["ManualOllama"], `${pathLabel}.ollama.mode`);
  requireNonEmptyString(ollama.title, `${pathLabel}.ollama.title`);
  requireNonEmptyString(ollama.detail, `${pathLabel}.ollama.detail`);
  requireFalse(ollama.automaticPulls, `${pathLabel}.ollama.automaticPulls`);
  const candidates = requireContractArray(ollama.candidates, `${pathLabel}.ollama.candidates`);
  if (candidates.length !== LOCAL_OLLAMA_SETUP_CANDIDATES.length) {
    throw new Error(
      `desktop_snapshot contract drift: expected ${pathLabel}.ollama.candidates to match the local-only candidate list`,
    );
  }
  candidates.forEach((candidate, index) => {
    const candidatePath = `${pathLabel}.ollama.candidates[${index}]`;
    const expected = LOCAL_OLLAMA_SETUP_CANDIDATES[index];
    const record = requireContractRecord(candidate, candidatePath);
    const id = requireNonEmptyString(record.id, `${candidatePath}.id`);
    if (id !== expected.id) {
      throw new Error(`desktop_snapshot contract drift: expected ${candidatePath}.id to be a local Ollama preset`);
    }
    requireNonEmptyString(record.displayName, `${candidatePath}.displayName`);
    const modelTag = requireNonEmptyString(record.modelTag, `${candidatePath}.modelTag`);
    if (modelTag !== expected.modelTag) {
      throw new Error(`desktop_snapshot contract drift: expected ${candidatePath}.modelTag to be a local model tag`);
    }
    const pullCommand = requireNonEmptyString(record.pullCommand, `${candidatePath}.pullCommand`);
    if (pullCommand !== `ollama pull ${modelTag}`) {
      throw new Error(`desktop_snapshot contract drift: expected ${candidatePath}.pullCommand to match modelTag`);
    }
    requireBoolean(record.defaultCandidate, `${candidatePath}.defaultCandidate`);
    requireNonEmptyString(record.setupNotes, `${candidatePath}.setupNotes`);
  });
}

function validateWhisperPathTestEvidence(value: unknown, pathLabel: string): void {
  if (value === null) {
    return;
  }
  const evidence = requireContractRecord(value, pathLabel);
  requireString(evidence.testedPath, `${pathLabel}.testedPath`);
  requireNonNegativeInteger(evidence.testedAtMs, `${pathLabel}.testedAtMs`);
  const state = requireEnum(evidence.state, ["Valid", "Invalid"], `${pathLabel}.state`);
  if (state === "Valid") {
    requirePositiveInteger(evidence.fileSizeBytes, `${pathLabel}.fileSizeBytes`);
    const sha256 = requireString(evidence.sha256, `${pathLabel}.sha256`);
    if (!/^[a-f0-9]{64}$/.test(sha256)) {
      throw new Error(`desktop_snapshot contract drift: expected ${pathLabel}.sha256 to be a SHA-256 hex string`);
    }
  } else {
    const fileSizeBytes = requireNullableNumber(evidence.fileSizeBytes, `${pathLabel}.fileSizeBytes`);
    if (fileSizeBytes !== null) {
      requireNonNegativeInteger(fileSizeBytes, `${pathLabel}.fileSizeBytes`);
    }
    const sha256 = requireNullableString(evidence.sha256, `${pathLabel}.sha256`);
    if (sha256 !== null && !/^[a-f0-9]{64}$/.test(sha256)) {
      throw new Error(`desktop_snapshot contract drift: expected ${pathLabel}.sha256 to be a SHA-256 hex string`);
    }
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

  if (modelKind !== "ready" && modelKind !== "untested" && modelKind !== "unsupported") {
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
    requirePositiveInteger(fileSizeBytes, `${evidencePath}.fileSizeBytes`);
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

export function assertWhisperModelPathTestContract(value: unknown): asserts value is WhisperModelPathTestResult {
  const result = requireContractRecord(value, "test_whisper_model_path");
  const state = requireEnum(result.state, ["Valid", "Invalid"], "test_whisper_model_path.state");
  requireString(result.message, "test_whisper_model_path.message");
  requireString(result.setupGuidance, "test_whisper_model_path.setupGuidance");

  if (state === "Valid") {
    requirePositiveInteger(result.fileSizeBytes, "test_whisper_model_path.fileSizeBytes");
    const sha256 = requireString(result.sha256, "test_whisper_model_path.sha256");
    if (!/^[a-f0-9]{64}$/.test(sha256)) {
      throw new Error("desktop_snapshot contract drift: expected test_whisper_model_path.sha256 to be a SHA-256 hex string");
    }
  }
}

export function assertOllamaConnectionTestContract(value: unknown): asserts value is OllamaConnectionTestResult {
  const result = requireContractRecord(value, "test_ollama_connection");
  requireEnum(result.state, ["Available", "Unavailable"], "test_ollama_connection.state");
  requireString(result.message, "test_ollama_connection.message");
  requireString(result.setupGuidance, "test_ollama_connection.setupGuidance");
  requireNullableString(result.selectedLocalModelTag, "test_ollama_connection.selectedLocalModelTag");
  requireNullableStringArray(result.installedLocalModels, "test_ollama_connection.installedLocalModels");
  requireNullableString(result.pullCommand, "test_ollama_connection.pullCommand");
}

export function assertMeetingSearchResultsContract(value: unknown): asserts value is MeetingSearchResult[] {
  requireContractArray(value, "search_meetings").forEach((result, index) => {
    const pathLabel = `search_meetings[${index}]`;
    const record = requireContractRecord(result, pathLabel);
    requireNonEmptyString(record.meeting_id, `${pathLabel}.meeting_id`);
    requireString(record.title, `${pathLabel}.title`);
  });
}
