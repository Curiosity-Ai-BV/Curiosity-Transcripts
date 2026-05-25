export type CommandRecordingState =
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

export interface WhisperModelPathTestResult {
  state: "Valid" | "Invalid";
  message: string;
  setupGuidance: string;
}

export interface OllamaConnectionTestResult {
  state: "Available" | "Unavailable";
  message: string;
  setupGuidance: string;
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
  exportCommand: ExportCommandState;
  deleteCommand: DeleteCommandState;
  analysisCommand: AnalysisCommandView | null;
}

export type CommandFetcher = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;

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
  requireContractArray(root.meetings, "desktop_snapshot.meetings").forEach((meeting, index) => {
    const meetingPath = `desktop_snapshot.meetings[${index}]`;
    for (const path of REQUIRED_MEETING_PATHS) {
      requireContractPath(meeting, path, meetingPath);
    }

    const meetingRecord = requireContractRecord(meeting, meetingPath);
    requireContractArray(meetingRecord.segments, `${meetingPath}.segments`).forEach(
      (segment, segmentIndex) => {
        const segmentPath = `${meetingPath}.segments[${segmentIndex}]`;
        for (const path of REQUIRED_SEGMENT_PATHS) {
          requireContractPath(segment, path, segmentPath);
        }
      },
    );
    requireNullableContractObject(
      meetingRecord.analysis,
      `${meetingPath}.analysis`,
      REQUIRED_ANALYSIS_DISCLOSURE_PATHS,
    );
  });

  requireNullableContractObject(
    root.transcription,
    "desktop_snapshot.transcription",
    REQUIRED_TRANSCRIPTION_PATHS,
  );
  requireNullableContractObject(
    root.analysisCommand,
    "desktop_snapshot.analysisCommand",
    REQUIRED_ANALYSIS_COMMAND_PATHS,
  );
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
    exportCommand: {
      state: "idle",
    },
    deleteCommand: {
      state: "idle",
    },
    analysisCommand: null,
  };
}

export function getUnavailableDesktopSnapshot(detail: string): DesktopSnapshot {
  return {
    loading: false,
    commandSurface: {
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
    exportCommand: {
      state: "idle",
    },
    deleteCommand: {
      state: "idle",
    },
    analysisCommand: null,
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
    sourceChannel,
    modelRunId: "run-1",
    transcriptVersionId: "version-1",
  };
}

type ContractPath = readonly string[];
type ContractRecord = Record<string, unknown>;

const REQUIRED_DESKTOP_SNAPSHOT_PATHS: readonly ContractPath[] = [
  ["loading"],
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
  ["exportCommand", "state"],
  ["deleteCommand", "state"],
  ["analysisCommand"],
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
  ["sourceChannel"],
  ["modelRunId"],
  ["transcriptVersionId"],
];

const REQUIRED_ANALYSIS_DISCLOSURE_PATHS: readonly ContractPath[] = [
  ["provider"],
  ["modelName"],
  ["networkUsed"],
  ["disclosureRequired"],
  ["disclosureConfirmed"],
  ["summary"],
  ["createdAtMs"],
  ["promptTemplateVersion"],
];

const REQUIRED_TRANSCRIPTION_PATHS: readonly ContractPath[] = [
  ["meetingId"],
  ["state"],
  ["failure"],
];

const REQUIRED_ANALYSIS_COMMAND_PATHS: readonly ContractPath[] = [
  ["meetingId"],
  ["state"],
  ["analysis"],
  ["failure"],
];

const REQUIRED_COMMAND_FAILURE_PATHS: readonly ContractPath[] = [
  ["code"],
  ["message"],
  ["setupGuidance"],
];

const REQUIRED_ANALYSIS_RESULT_PATHS: readonly ContractPath[] = [
  ["provider"],
  ["modelName"],
  ["networkUsed"],
  ["summary"],
];

const DESKTOP_SNAPSHOT_COMMANDS = new Set([
  "desktop_snapshot",
  "delete_meeting",
  "export_meeting_json",
  "generate_summary",
  "rename_meeting",
  "save_analysis_settings",
  "save_whisper_model_path",
  "seed_dev_fixture",
  "start_microphone_recording",
  "stop_microphone_recording",
  "transcribe_meeting",
]);

function requireNullableContractObject(
  value: unknown,
  pathLabel: string,
  requiredPaths: readonly ContractPath[],
) {
  if (value === null) {
    return;
  }
  for (const path of requiredPaths) {
    requireContractPath(value, path, pathLabel);
  }

  const record = requireContractRecord(value, pathLabel);
  if (Object.prototype.hasOwnProperty.call(record, "failure")) {
    requireNullableContractObject(
      record.failure,
      `${pathLabel}.failure`,
      REQUIRED_COMMAND_FAILURE_PATHS,
    );
  }
  if (Object.prototype.hasOwnProperty.call(record, "analysis")) {
    requireNullableContractObject(
      record.analysis,
      `${pathLabel}.analysis`,
      REQUIRED_ANALYSIS_RESULT_PATHS,
    );
  }
}

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
    return result as T;
  };
}
