import { afterEach, describe, expect, it, vi } from "vitest";

import desktopCommandViewContract from "../contracts/desktop-command-view-contract.fixture.json";
import {
  CommandFetcher,
  createDesktopCommandFacade,
  getDesktopCommandFetcher,
  getMockDesktopSnapshot,
  loadDesktopSnapshot,
} from "./commandAdapter";

type DesktopCommandViewContractFixture = {
  version: number;
  owner: string;
  cases: Record<string, unknown>;
};

const rustContractFixture = desktopCommandViewContract as DesktopCommandViewContractFixture;

const tauriInvoke = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({
  invoke: tauriInvoke,
}));

afterEach(() => {
  tauriInvoke.mockReset();
  delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
});

describe("desktop snapshot DTO contract", () => {
  it.each([
    "desktop_snapshot.empty",
    "desktop_snapshot.transcribed_analyzed_meeting",
    "desktop_snapshot.with_setup_evidence",
  ])(
    "accepts the Rust-serialized %s fixture",
    async (caseName) => {
      const fixtureCase = rustContractFixture.cases[caseName];
      const fetchCommand: CommandFetcher = async (command) => {
        expect(command).toBe("desktop_snapshot");
        return fixtureCase as never;
      };

      await expect(
        loadDesktopSnapshot({
          fetchCommand,
          previewFallback: false,
        }),
      ).resolves.toEqual(fixtureCase);
    },
  );

  it("accepts recovered and retryable command jobs with optional failure detail", async () => {
    const backendSnapshot = {
      ...getMockDesktopSnapshot(),
      transcriptionJob: {
        id: "transcription-circuit-review-1700000001000",
        kind: "Transcription",
        meetingId: "circuit-review",
        state: "Recovery",
        cancelRequested: false,
        startedAtMs: 1_700_000_001_000,
        lastError: "transcription worker was not running after app restart",
      },
      summaryJob: {
        id: "summary-circuit-review-1700000002000",
        kind: "Summary",
        meetingId: "circuit-review",
        state: "Retry",
        cancelRequested: false,
        startedAtMs: 1_700_000_002_000,
      },
    };
    const fetchCommand: CommandFetcher = async () => backendSnapshot as never;

    await expect(
      loadDesktopSnapshot({
        fetchCommand,
        previewFallback: false,
      }),
    ).resolves.toEqual(backendSnapshot);
  });

  it("requires first-run setup guidance to be explicit in snapshots", async () => {
    const backendSnapshot = getMockDesktopSnapshot();
    const driftedBackendSnapshot = {
      ...backendSnapshot,
    } as Record<string, unknown>;
    delete driftedBackendSnapshot.setupGuidance;
    const fetchCommand: CommandFetcher = async () => driftedBackendSnapshot as never;

    await expect(
      loadDesktopSnapshot({
        fetchCommand,
        previewFallback: false,
      }),
    ).rejects.toThrow("desktop_snapshot.setupGuidance");
  });

  it("guards setup guidance state and unknown Ollama availability in snapshots", async () => {
    const backendSnapshot = getMockDesktopSnapshot();
    const driftedBackendSnapshot = {
      ...backendSnapshot,
      setupGuidance: {
        ...backendSnapshot.setupGuidance,
        whisper: {
          ...backendSnapshot.setupGuidance.whisper,
          state: "Compatible",
        },
        ollama: {
          ...backendSnapshot.setupGuidance.ollama,
          availability: "Available",
        },
      },
    };
    const fetchCommand: CommandFetcher = async () => driftedBackendSnapshot as never;

    await expect(
      loadDesktopSnapshot({
        fetchCommand,
        previewFallback: false,
      }),
    ).rejects.toThrow("desktop_snapshot.setupGuidance.whisper.state");
  });

  it("guards setup guidance evidence field types in snapshots", async () => {
    const backendSnapshot = getMockDesktopSnapshot();
    const driftedBackendSnapshot = {
      ...backendSnapshot,
      setupGuidance: {
        ...backendSnapshot.setupGuidance,
        whisper: {
          ...backendSnapshot.setupGuidance.whisper,
          lastPathTest: {
            testedPath: "/models/base.en.bin",
            testedAtMs: "later",
            state: "Valid",
            fileSizeBytes: 16,
            sha256: "8b68af71d2eaaec61d5b4f50e330493cc0074323676962d9761cbc7c6810ba54",
            failureDetail: null,
          },
        },
        ollama: {
          ...backendSnapshot.setupGuidance.ollama,
          lastConnectionTest: {
            baseUrl: "http://127.0.0.1:11434",
            requestedModel: "qwen3.6:27b",
            testedAtMs: 1_700_000_002_000,
            state: "Available",
            selectedLocalModelTag: "qwen3.6:27b",
            installedLocalModels: ["qwen3.6:27b"],
            pullCommand: null,
            failureDetail: null,
          },
        },
      },
    };
    const fetchCommand: CommandFetcher = async () => driftedBackendSnapshot as never;

    await expect(
      loadDesktopSnapshot({
        fetchCommand,
        previewFallback: false,
      }),
    ).rejects.toThrow("desktop_snapshot.setupGuidance.whisper.lastPathTest.testedAtMs");
  });

  it("requires calendar context to be explicit and non-recording in snapshots", async () => {
    const backendSnapshot = getMockDesktopSnapshot();
    const missingCalendarContext = {
      ...backendSnapshot,
    } as Record<string, unknown>;
    delete missingCalendarContext.calendarContext;
    const missingFetchCommand: CommandFetcher = async () => missingCalendarContext as never;

    await expect(
      loadDesktopSnapshot({
        fetchCommand: missingFetchCommand,
        previewFallback: false,
      }),
    ).rejects.toThrow("desktop_snapshot.calendarContext");

    const autoStartDrift = {
      ...backendSnapshot,
      calendarContext: {
        ...backendSnapshot.calendarContext,
        autoStartEnabled: true,
      },
    };
    const autoStartFetchCommand: CommandFetcher = async () => autoStartDrift as never;

    await expect(
      loadDesktopSnapshot({
        fetchCommand: autoStartFetchCommand,
        previewFallback: false,
      }),
    ).rejects.toThrow("desktop_snapshot.calendarContext.autoStartEnabled");
  });

  it("guards calendar context event safety fields in snapshots", async () => {
    const backendSnapshot = getMockDesktopSnapshot();
    const driftedBackendSnapshot = {
      ...backendSnapshot,
      calendarContext: {
        ...backendSnapshot.calendarContext,
        availabilityState: "Ready",
        permissionState: "Granted",
        upcomingEvents: [
          {
            id: "event-1",
            title: "Planning Review",
            calendarTitle: "Work",
            startsAtMs: 1_700_000_000_000,
            endsAtMs: 1_700_000_900_000,
            isAllDay: false,
            isRecurring: false,
            privacy: "Secret",
            overlapState: "None",
            attachable: true,
            safetyNote: "Manual attach allowed.",
          },
        ],
      },
    };
    const fetchCommand: CommandFetcher = async () => driftedBackendSnapshot as never;

    await expect(
      loadDesktopSnapshot({
        fetchCommand,
        previewFallback: false,
      }),
    ).rejects.toThrow("desktop_snapshot.calendarContext.upcomingEvents[0].privacy");
  });

  it("fails loudly when a backend snapshot omits a frontend-required recording field", async () => {
    const backendSnapshot = getMockDesktopSnapshot();
    const driftedBackendSnapshot = {
      ...backendSnapshot,
      recording: {
        ...backendSnapshot.recording,
      },
    } as Record<string, unknown>;
    delete (driftedBackendSnapshot.recording as Record<string, unknown>).permission_state;

    const fetchCommand: CommandFetcher = async () => driftedBackendSnapshot as never;

    await expect(
      loadDesktopSnapshot({
        fetchCommand,
        previewFallback: false,
      }),
    ).rejects.toThrow("desktop_snapshot.recording.permission_state");
  });

  it("validates snapshot-returning command results from the Tauri command fetcher", async () => {
    (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {};
    const backendSnapshot = getMockDesktopSnapshot();
    const driftedBackendSnapshot = {
      ...backendSnapshot,
      model: {
        kind: backendSnapshot.model.kind,
      },
    };
    tauriInvoke.mockResolvedValue(driftedBackendSnapshot);

    const fetchCommand = getDesktopCommandFetcher();

    expect(fetchCommand).toBeDefined();
    await expect(fetchCommand!("start_microphone_recording")).rejects.toThrow(
      "desktop_snapshot.model.configuredPath",
    );
  });

  it("requires command readiness to be explicit instead of inferred from detail text", async () => {
    const backendSnapshot = getMockDesktopSnapshot();
    const driftedBackendSnapshot = {
      ...backendSnapshot,
      commandSurface: {
        detail: "Connected to local desktop commands.",
      },
    };
    const fetchCommand: CommandFetcher = async () => driftedBackendSnapshot as never;

    await expect(
      loadDesktopSnapshot({
        fetchCommand,
        previewFallback: false,
      }),
    ).rejects.toThrow("desktop_snapshot.commandSurface.ready");
  });

  it("requires transcript segments to carry original transcript text explicitly", async () => {
    const backendSnapshot = getMockDesktopSnapshot();
    const driftedBackendSnapshot = {
      ...backendSnapshot,
      meetings: backendSnapshot.meetings.map((meeting) => ({
        ...meeting,
        segments: meeting.segments.map((segment) => ({
          ...segment,
          originalText: null,
        })),
      })),
    };
    delete (driftedBackendSnapshot.meetings[0].segments[0] as Record<string, unknown>).originalText;
    const fetchCommand: CommandFetcher = async () => driftedBackendSnapshot as never;

    await expect(
      loadDesktopSnapshot({
        fetchCommand,
        previewFallback: false,
      }),
    ).rejects.toThrow("desktop_snapshot.meetings[0].segments[0].originalText");
  });

  it.each([
    ["desktop_snapshot.loading", { loading: "false" }],
    ["desktop_snapshot.model.kind", { model: { ...getMockDesktopSnapshot().model, kind: "warm" } }],
    ["desktop_snapshot.recording.state", { recording: { ...getMockDesktopSnapshot().recording, state: "Started" } }],
    [
      "desktop_snapshot.recording.recoverable",
      { recording: { ...getMockDesktopSnapshot().recording, recoverable: null } },
    ],
    [
      "desktop_snapshot.capture.microphone",
      { capture: { ...getMockDesktopSnapshot().capture, microphone: "Allowed" } },
    ],
    [
      "desktop_snapshot.meetings[0].privacy.localOnly",
      {
        meetings: [
          {
            ...getMockDesktopSnapshot().meetings[0],
            privacy: { ...getMockDesktopSnapshot().meetings[0].privacy, localOnly: "yes" },
          },
        ],
      },
    ],
    [
      "desktop_snapshot.meetings[0].analysis.networkUsed",
      {
        meetings: [
          {
            ...getMockDesktopSnapshot().meetings[0],
            analysis: {
              provider: "ollama",
              modelName: "qwen3.6:27b",
              networkUsed: "false",
              disclosureRequired: false,
              disclosureConfirmed: false,
              summary: "Local summary",
              createdAtMs: 1_700_000_002_000,
              promptTemplateVersion: "summary-v1",
            },
          },
        ],
      },
    ],
    [
      "desktop_snapshot.exportCommand.format",
      {
        exportCommand: {
          state: "exported",
          meetingId: "circuit-review",
          path: "/tmp/circuit-review.md",
        },
      },
    ],
    [
      "desktop_snapshot.exportCommand.format",
      {
        exportCommand: {
          state: "exported",
          meetingId: "circuit-review",
          format: "pdf",
          path: "/tmp/circuit-review.pdf",
        },
      },
    ],
    [
      "desktop_snapshot.transcription.state",
      {
        transcription: {
          meetingId: "circuit-review",
          state: "Done",
          failure: null,
        },
      },
    ],
    [
      "desktop_snapshot.transcriptionJob.cancelRequested",
      {
        transcriptionJob: {
          id: "transcription-circuit-review-1700000001000",
          kind: "Transcription",
          meetingId: "circuit-review",
          state: "CancelRequested",
          cancelRequested: "true",
          startedAtMs: 1_700_000_001_000,
        },
      },
    ],
    [
      "desktop_snapshot.summaryJob.startedAtMs",
      {
        summaryJob: {
          id: "summary-circuit-review-1700000001000",
          kind: "Summary",
          meetingId: "circuit-review",
          state: "Running",
          cancelRequested: false,
          startedAtMs: "1700000001000",
        },
      },
    ],
    [
      "desktop_snapshot.analysisCommand.failure.setupGuidance",
      {
        analysisCommand: {
          meetingId: "circuit-review",
          state: "Failed",
          analysis: null,
          failure: {
            code: "ollama_unavailable",
            message: "Ollama is unavailable.",
            setupGuidance: null,
          },
        },
      },
    ],
    [
      "desktop_snapshot.analysisCommand.analysis.networkUsed",
      {
        analysisCommand: {
          meetingId: "circuit-review",
          state: "Complete",
          analysis: {
            provider: "ollama",
            modelName: "qwen3.6:27b",
            networkUsed: "yes",
            summary: "Local summary",
          },
          failure: null,
        },
      },
    ],
    [
      "desktop_snapshot.setupGuidance.ollama.availability",
      {
        setupGuidance: {
          ...getMockDesktopSnapshot().setupGuidance,
          ollama: {
            ...getMockDesktopSnapshot().setupGuidance.ollama,
            availability: "Reachable",
          },
        },
      },
    ],
  ])("fails loudly when %s has the wrong runtime type or enum value", async (path, patch) => {
    const backendSnapshot = getMockDesktopSnapshot();
    const driftedBackendSnapshot = {
      ...backendSnapshot,
      ...patch,
    };
    const fetchCommand: CommandFetcher = async () => driftedBackendSnapshot as never;

    await expect(
      loadDesktopSnapshot({
        fetchCommand,
        previewFallback: false,
      }),
    ).rejects.toThrow(path);
  });
});

describe("typed desktop command facade", () => {
  it.each([
    ["test_whisper_model_path.valid_readable_file", "testWhisperModelPath", { path: "<app-root>/fixture-whisper.bin" }],
    ["test_whisper_model_path.missing_path", "testWhisperModelPath", { path: "" }],
    [
      "test_ollama_connection.available_configured_model",
      "testOllamaConnection",
      { baseUrl: "http://127.0.0.1:11434", model: "qwen3.6:27b" },
    ],
    [
      "test_ollama_connection.missing_local_model",
      "testOllamaConnection",
      { baseUrl: "http://127.0.0.1:11434", model: "qwen3.6:27b" },
    ],
    [
      "test_ollama_connection.cloud_model_rejected",
      "testOllamaConnection",
      { baseUrl: "http://127.0.0.1:11434", model: "deepseek-v3.2:cloud" },
    ],
  ] as const)("accepts the Rust-serialized %s fixture through the facade", async (caseName, method, args) => {
    const fixtureCase = rustContractFixture.cases[caseName];
    const facade = createDesktopCommandFacade(async () => fixtureCase as never);

    await expect(facade[method](args as never)).resolves.toEqual(fixtureCase);
  });

  it("maps typed facade methods through production command names and args", async () => {
    const calls: Array<{ command: string; args?: Record<string, unknown> }> = [];
    const snapshot = getMockDesktopSnapshot();
    const fetchCommand: CommandFetcher = async (command, args) => {
      calls.push({ command, args });
      if (command === "test_whisper_model_path") {
        return {
          state: "Valid",
          message: "Whisper model path is readable.",
          setupGuidance: "",
          fileSizeBytes: 16,
          sha256: "8b68af71d2eaaec61d5b4f50e330493cc0074323676962d9761cbc7c6810ba54",
        } as never;
      }
      if (command === "test_ollama_connection") {
        return {
          state: "Available",
          message: "Ollama is reachable.",
          setupGuidance: "",
          selectedLocalModelTag: "qwen3.6:27b",
          installedLocalModels: ["qwen3.6:27b"],
          pullCommand: null,
        } as never;
      }
      if (command === "search_meetings") {
        return [{ meeting_id: "circuit-review", title: "Circuit Review" }] as never;
      }
      return snapshot as never;
    };

    const facade = createDesktopCommandFacade(fetchCommand);

    await facade.desktopSnapshot();
    await facade.searchMeetings({ query: "retention" });
    await facade.startRecording({ title: "MVP sync" });
    await facade.importAudioFile({ sourcePath: "/Users/adrian/imports/customer-call.wav", title: "Imported call" });
    await facade.stopRecording();
    await facade.transcribeMeeting({ meetingId: "circuit-review" });
    await facade.correctTranscriptSegment({
      meetingId: "circuit-review",
      segmentId: "segment-1",
      correctedText: "Corrected transcript text.",
      editedAtMs: 1_700_000_003_000,
    });
    await facade.cancelTranscription({ jobId: "transcription-circuit-review-1700000001000" });
    await facade.renameMeeting({ meetingId: "circuit-review", title: "Renamed Planning" });
    await facade.exportMeeting({ meetingId: "circuit-review", format: "markdown" });
    await facade.exportMeetingJson({ meetingId: "circuit-review" });
    await facade.deleteMeeting({ meetingId: "circuit-review" });
    await facade.generateSummary({ meetingId: "circuit-review" });
    await facade.cancelSummary({ jobId: "summary-circuit-review-1700000001000" });
    await facade.saveWhisperModelPath({ whisperModelPath: "/models/base.en.bin" });
    await facade.saveAnalysisSettings({ ollamaBaseUrl: "http://127.0.0.1:11434", ollamaModel: "qwen3.6:27b" });
    await facade.saveRawAudioRetentionPolicy({ rawAudioRetentionPolicy: "DeleteAfterTranscription" });
    await facade.requestAppleCalendarAccess();
    const whisperPathTest = await facade.testWhisperModelPath({ path: "/models/base.en.bin" });
    await facade.testOllamaConnection({ baseUrl: "http://127.0.0.1:11434", model: "qwen3.6:27b" });

    expect(calls).toEqual([
      { command: "desktop_snapshot", args: undefined },
      { command: "search_meetings", args: { query: "retention" } },
      {
        command: "start_microphone_recording",
        args: { title: "MVP sync" },
      },
      {
        command: "import_audio_file",
        args: { sourcePath: "/Users/adrian/imports/customer-call.wav", title: "Imported call" },
      },
      { command: "stop_microphone_recording", args: undefined },
      { command: "transcribe_meeting", args: { meetingId: "circuit-review" } },
      {
        command: "correct_transcript_segment",
        args: {
          meetingId: "circuit-review",
          segmentId: "segment-1",
          correctedText: "Corrected transcript text.",
          editedAtMs: 1_700_000_003_000,
        },
      },
      {
        command: "cancel_transcription",
        args: { jobId: "transcription-circuit-review-1700000001000" },
      },
      { command: "rename_meeting", args: { meetingId: "circuit-review", title: "Renamed Planning" } },
      { command: "export_meeting", args: { meetingId: "circuit-review", format: "markdown" } },
      { command: "export_meeting_json", args: { meetingId: "circuit-review" } },
      { command: "delete_meeting", args: { meetingId: "circuit-review" } },
      { command: "generate_summary", args: { meetingId: "circuit-review" } },
      { command: "cancel_summary", args: { jobId: "summary-circuit-review-1700000001000" } },
      { command: "save_whisper_model_path", args: { whisperModelPath: "/models/base.en.bin" } },
      {
        command: "save_analysis_settings",
        args: { ollamaBaseUrl: "http://127.0.0.1:11434", ollamaModel: "qwen3.6:27b" },
      },
      {
        command: "save_raw_audio_retention_policy",
        args: { rawAudioRetentionPolicy: "DeleteAfterTranscription" },
      },
      { command: "request_apple_calendar_access", args: undefined },
      { command: "test_whisper_model_path", args: { path: "/models/base.en.bin" } },
      {
        command: "test_ollama_connection",
        args: { baseUrl: "http://127.0.0.1:11434", model: "qwen3.6:27b" },
      },
    ]);
    expect(whisperPathTest).toMatchObject({
      fileSizeBytes: 16,
      sha256: "8b68af71d2eaaec61d5b4f50e330493cc0074323676962d9761cbc7c6810ba54",
    });
  });

  it("accepts legacy NeverSave raw-audio retention values from recording and privacy DTO snapshots", async () => {
    const backendSnapshot = getMockDesktopSnapshot();
    const snapshot = await loadDesktopSnapshot({
      fetchCommand: async <T,>(): Promise<T> =>
        ({
          ...backendSnapshot,
          recording: {
            ...backendSnapshot.recording,
            raw_audio_retention: "NeverSave",
          },
          meetings: backendSnapshot.meetings.map((meeting, index) =>
            index === 0
              ? {
                  ...meeting,
                  privacy: {
                    ...meeting.privacy,
                    rawAudioRetention: "NeverSave",
                  },
                }
              : meeting,
          ),
        }) as T,
      previewFallback: false,
    });

    expect(snapshot.recording.raw_audio_retention).toBe("NeverSave");
    expect(snapshot.meetings[0].privacy.rawAudioRetention).toBe("NeverSave");
  });

  it("rejects unsupported NeverSave raw-audio retention values from settings snapshots", async () => {
    const backendSnapshot = getMockDesktopSnapshot();
    await expect(
      loadDesktopSnapshot({
        fetchCommand: async <T,>(): Promise<T> =>
          ({
            ...backendSnapshot,
            settings: {
              ...backendSnapshot.settings,
              rawAudioRetentionPolicy: "NeverSave",
            },
          }) as T,
        previewFallback: false,
      }),
    ).rejects.toThrow("desktop_snapshot.settings.rawAudioRetentionPolicy");
  });

  it("validates snapshot-returning facade commands before exposing them to App", async () => {
    const backendSnapshot = getMockDesktopSnapshot();
    const driftedBackendSnapshot = {
      ...backendSnapshot,
      recording: {
        ...backendSnapshot.recording,
      },
    } as Record<string, unknown>;
    delete (driftedBackendSnapshot.recording as Record<string, unknown>).permission_state;
    const facade = createDesktopCommandFacade(async () => driftedBackendSnapshot as never);

    await expect(facade.stopRecording()).rejects.toThrow("desktop_snapshot.recording.permission_state");
  });

  it("fails loudly when a valid Whisper path test omits readable file metadata", async () => {
    const facade = createDesktopCommandFacade(async (command) => {
      if (command === "test_whisper_model_path") {
        return {
          state: "Valid",
          message: "Whisper model path is readable.",
          setupGuidance: "",
        } as never;
      }
      return getMockDesktopSnapshot() as never;
    });

    await expect(facade.testWhisperModelPath({ path: "/models/base.en.bin" })).rejects.toThrow(
      "test_whisper_model_path.fileSizeBytes",
    );
  });

  it("fails loudly when a reachable Ollama test omits setup metadata", async () => {
    const facade = createDesktopCommandFacade(async (command) => {
      if (command === "test_ollama_connection") {
        return {
          state: "Available",
          message: "Ollama is reachable.",
          setupGuidance: "",
        } as never;
      }
      return getMockDesktopSnapshot() as never;
    });

    await expect(
      facade.testOllamaConnection({ baseUrl: "http://127.0.0.1:11434", model: "qwen3.6:27b" }),
    ).rejects.toThrow("test_ollama_connection.selectedLocalModelTag");
  });
});
