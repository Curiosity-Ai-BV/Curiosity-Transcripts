import {
  assertDesktopSnapshotContract,
  assertMeetingSearchResultsContract,
  assertOllamaConnectionTestContract,
  assertWhisperModelPathTestContract,
} from "./desktopContract";

import type {
  AppPermissionState,
  RawAudioRetentionPolicy,
  PersistedRawAudioRetentionPolicy,
  ExportFormat,
  CommandRecordingDto,
  StatusView,
  ModelStatus,
  ModelSetupOptions,
  WhisperModelPathTestResult,
  OllamaConnectionTestResult,
  ExportCommandState,
  DeleteCommandState,
  MeetingSearchResult,
  AnalysisDisclosureState,
  TranscriptionCommandView,
  TranscriptSegment,
  MeetingView,
  CommandJobView,
  DesktopSnapshot,
} from "./desktopContract";

export {
  assertDesktopSnapshotContract,
} from "./desktopContract";

export type {
  CommandRecordingState,
  AppPermissionState,
  RawAudioRetentionPolicy,
  PersistedRawAudioRetentionPolicy,
  Tone,
  ExportFormat,
  CommandRecordingDto,
  StatusView,
  CommandSurfaceState,
  ModelStatus,
  WhisperSetupState,
  OllamaSetupState,
  OllamaAvailabilityState,
  WhisperSetupGuidance,
  OllamaSetupGuidance,
  FirstRunSetupGuidance,
  ModelSetupOptions,
  WhisperModelSetupOptions,
  OllamaModelSetupOptions,
  OllamaModelSetupCandidate,
  CalendarPermissionState,
  CalendarAvailabilityState,
  CalendarEventPrivacy,
  CalendarEventOverlapState,
  CalendarContextEvent,
  CalendarContext,
  MeetingCalendarAttachment,
  AppSettings,
  WhisperPathTestEvidence,
  WhisperTranscriptionCompatibilityEvidence,
  WhisperModelPathTestResult,
  OllamaConnectionTestResult,
  OllamaConnectionTestEvidence,
  ExportCommandState,
  DeleteCommandState,
  MeetingSearchResult,
  AnalysisDisclosureState,
  CommandFailureView,
  AnalysisCommandView,
  TranscriptionCommandView,
  TranscriptSegment,
  MeetingView,
  CaptureStatus,
  CommandJobView,
  DesktopSnapshot,
} from "./desktopContract";
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
  if (model.kind === "unsupported") {
    return {
      label: "Whisper file unsupported",
      tone: "blocked",
      detail: "Choose a supported .bin or .gguf Whisper model file before transcription.",
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

function defaultModelSetupOptions(): ModelSetupOptions {
  return {
    whisper: {
      mode: "ManualFile",
      title: "Local Whisper file",
      detail:
        "Choose an existing whisper.cpp-compatible .bin or .gguf model file. Curiosity does not download Whisper models yet.",
      chooseLabel: "Choose model",
      saveLabel: "Save Whisper",
      testLabel: "Test path",
      downloadsManaged: false,
      acceptedExtensions: ["bin", "gguf"],
    },
    ollama: {
      mode: "ManualOllama",
      title: "Local Ollama models",
      detail:
        "Start Ollama locally and install one of the listed local model tags manually before running Test Ollama.",
      automaticPulls: false,
      candidates: [
        {
          id: "ollama-qwen3-6-27b",
          displayName: "Qwen 3.6 27B",
          modelTag: "qwen3.6:27b",
          pullCommand: "ollama pull qwen3.6:27b",
          defaultCandidate: true,
          setupNotes: "Install Ollama locally, then run `ollama pull qwen3.6:27b`.",
        },
        {
          id: "ollama-gemma4-31b",
          displayName: "Gemma 4 31B",
          modelTag: "gemma4:31b",
          pullCommand: "ollama pull gemma4:31b",
          defaultCandidate: true,
          setupNotes: "Install Ollama locally, then run `ollama pull gemma4:31b`.",
        },
      ],
    },
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
    modelSetupOptions: defaultModelSetupOptions(),
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
    modelSetupOptions: defaultModelSetupOptions(),
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
