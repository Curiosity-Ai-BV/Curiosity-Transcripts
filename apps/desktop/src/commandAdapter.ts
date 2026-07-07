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
export type Tone = "ready" | "active" | "warn" | "blocked" | "muted";

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
  kind: "ready" | "missing" | "transcribing";
  configuredPath: string;
}

export interface AppSettings {
  whisperModelPath: string;
  ollamaBaseUrl: string;
  ollamaModel: string;
  exportDirectory: string | null;
}

interface WhisperModelPathTestBase {
  message: string;
  setupGuidance: string;
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

export interface ExportCommandState {
  state: "idle" | "exporting" | "exported" | "failed";
  meetingId?: string | null;
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
  state: "Running" | "CancelRequested" | "Complete" | "Failed" | "Canceled";
  cancelRequested: boolean;
  startedAtMs: number;
}

export interface DesktopSnapshot {
  loading: boolean;
  commandSurface: CommandSurfaceState;
  meetings: MeetingView[];
  selectedMeetingId: string | null;
  recording: CommandRecordingDto;
  model: ModelStatus;
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
  exportMeetingJson(args: { meetingId: string }): Promise<DesktopSnapshot>;
  deleteMeeting(args: { meetingId: string }): Promise<DesktopSnapshot>;
  generateSummary(args: { meetingId: string }): Promise<DesktopSnapshot>;
  cancelSummary(args: { jobId: string }): Promise<DesktopSnapshot>;
  saveWhisperModelPath(args: { whisperModelPath: string }): Promise<DesktopSnapshot>;
  saveAnalysisSettings(args: { ollamaBaseUrl: string; ollamaModel: string }): Promise<DesktopSnapshot>;
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
      ["Retain", "DeleteAfterTranscription", "NeverSave"],
      `${meetingPath}.privacy.rawAudioRetention`,
    );
    requireBoolean(privacy.localOnly, `${meetingPath}.privacy.localOnly`);
    validateExportCommandState(meetingRecord.exportState, `${meetingPath}.exportState`);
    validateDeleteCommandState(meetingRecord.deleteState, `${meetingPath}.deleteState`);
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
    ["Retain", "DeleteAfterTranscription", "NeverSave"],
    "desktop_snapshot.recording.raw_audio_retention",
  );
  requireBoolean(recording.recoverable, "desktop_snapshot.recording.recoverable");
  requireString(recording.recovery_action, "desktop_snapshot.recording.recovery_action");

  const model = requireContractRecord(root.model, "desktop_snapshot.model");
  requireEnum(model.kind, ["ready", "missing", "transcribing"], "desktop_snapshot.model.kind");
  requireString(model.configuredPath, "desktop_snapshot.model.configuredPath");

  const settings = requireContractRecord(root.settings, "desktop_snapshot.settings");
  requireString(settings.whisperModelPath, "desktop_snapshot.settings.whisperModelPath");
  requireString(settings.ollamaBaseUrl, "desktop_snapshot.settings.ollamaBaseUrl");
  requireString(settings.ollamaModel, "desktop_snapshot.settings.ollamaModel");
  requireNullableString(settings.exportDirectory, "desktop_snapshot.settings.exportDirectory");

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
      detail: `${job.meetingId} / ${job.id}`,
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
  if (state.state === "exported") {
    return {
      label: "JSON exported",
      tone: "ready",
      detail: state.path || "Export path recorded.",
    };
  }
  if (state.state === "exporting") {
    return {
      label: "Exporting",
      tone: "active",
      detail: "Writing a user-requested JSON export.",
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
    settings: {
      whisperModelPath:
        variant === "state-matrix" ? "~/Library/Application Support/Curiosity/models/base.en.bin" : "",
      ollamaBaseUrl: "http://127.0.0.1:11434",
      ollamaModel: "qwen3.6:27b",
      exportDirectory: null,
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
    settings: {
      whisperModelPath: "",
      ollamaBaseUrl: "http://127.0.0.1:11434",
      ollamaModel: "qwen3.6:27b",
      exportDirectory: null,
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

function meeting(input: Omit<MeetingView, "status" | "privacy" | "exportState" | "deleteState">): MeetingView {
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
  ["settings", "whisperModelPath"],
  ["settings", "ollamaBaseUrl"],
  ["settings", "ollamaModel"],
  ["settings", "exportDirectory"],
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
  "export_meeting_json",
  "generate_summary",
  "cancel_summary",
  "rename_meeting",
  "save_analysis_settings",
  "save_whisper_model_path",
  "seed_dev_fixture",
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

function validateExportCommandState(value: unknown, pathLabel: string): void {
  const state = requireContractRecord(value, pathLabel);
  requireEnum(state.state, ["idle", "exporting", "exported", "failed"], `${pathLabel}.state`);
  if (Object.prototype.hasOwnProperty.call(state, "meetingId")) {
    requireNullableString(state.meetingId, `${pathLabel}.meetingId`);
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
  requireEnum(job.state, ["Running", "CancelRequested", "Complete", "Failed", "Canceled"], `${pathLabel}.state`);
  requireBoolean(job.cancelRequested, `${pathLabel}.cancelRequested`);
  requireNumber(job.startedAtMs, `${pathLabel}.startedAtMs`);
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

function retentionDetail(policy: RawAudioRetentionPolicy): string {
  if (policy === "DeleteAfterTranscription") {
    return "Raw audio will be deleted after transcription.";
  }
  if (policy === "NeverSave") {
    return "Raw audio is not saved for this meeting.";
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
    searchMeetings: ({ query }) => fetchCommand<MeetingSearchResult[]>("search_meetings", { query }),
    startRecording: (args) =>
      snapshotCommand("start_microphone_recording", args?.title ? { title: args.title } : undefined),
    stopRecording: () => snapshotCommand("stop_microphone_recording"),
    transcribeMeeting: ({ meetingId }) => snapshotCommand("transcribe_meeting", { meetingId }),
    correctTranscriptSegment: ({ meetingId, segmentId, correctedText, editedAtMs }) =>
      snapshotCommand("correct_transcript_segment", { meetingId, segmentId, correctedText, editedAtMs }),
    cancelTranscription: ({ jobId }) => snapshotCommand("cancel_transcription", { jobId }),
    renameMeeting: ({ meetingId, title }) => snapshotCommand("rename_meeting", { meetingId, title }),
    exportMeetingJson: ({ meetingId }) => snapshotCommand("export_meeting_json", { meetingId }),
    deleteMeeting: ({ meetingId }) => snapshotCommand("delete_meeting", { meetingId }),
    generateSummary: ({ meetingId }) => snapshotCommand("generate_summary", { meetingId }),
    cancelSummary: ({ jobId }) => snapshotCommand("cancel_summary", { jobId }),
    saveWhisperModelPath: ({ whisperModelPath }) =>
      snapshotCommand("save_whisper_model_path", { whisperModelPath }),
    saveAnalysisSettings: ({ ollamaBaseUrl, ollamaModel }) =>
      snapshotCommand("save_analysis_settings", { ollamaBaseUrl, ollamaModel }),
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
