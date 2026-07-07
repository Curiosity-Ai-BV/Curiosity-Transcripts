import { afterEach, describe, expect, it, vi } from "vitest";

import {
  CommandFetcher,
  createDesktopCommandFacade,
  getDesktopCommandFetcher,
  getMockDesktopSnapshot,
  loadDesktopSnapshot,
} from "./commandAdapter";

const tauriInvoke = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({
  invoke: tauriInvoke,
}));

afterEach(() => {
  tauriInvoke.mockReset();
  delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
});

describe("desktop snapshot DTO contract", () => {
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
    await facade.stopRecording();
    await facade.transcribeMeeting({ meetingId: "circuit-review" });
    await facade.cancelTranscription({ jobId: "transcription-circuit-review-1700000001000" });
    await facade.renameMeeting({ meetingId: "circuit-review", title: "Renamed Planning" });
    await facade.exportMeetingJson({ meetingId: "circuit-review" });
    await facade.deleteMeeting({ meetingId: "circuit-review" });
    await facade.generateSummary({ meetingId: "circuit-review" });
    await facade.cancelSummary({ jobId: "summary-circuit-review-1700000001000" });
    await facade.saveWhisperModelPath({ whisperModelPath: "/models/base.en.bin" });
    await facade.saveAnalysisSettings({ ollamaBaseUrl: "http://127.0.0.1:11434", ollamaModel: "qwen3.6:27b" });
    const whisperPathTest = await facade.testWhisperModelPath({ path: "/models/base.en.bin" });
    await facade.testOllamaConnection({ baseUrl: "http://127.0.0.1:11434", model: "qwen3.6:27b" });

    expect(calls).toEqual([
      { command: "desktop_snapshot", args: undefined },
      { command: "search_meetings", args: { query: "retention" } },
      {
        command: "start_microphone_recording",
        args: { title: "MVP sync" },
      },
      { command: "stop_microphone_recording", args: undefined },
      { command: "transcribe_meeting", args: { meetingId: "circuit-review" } },
      {
        command: "cancel_transcription",
        args: { jobId: "transcription-circuit-review-1700000001000" },
      },
      { command: "rename_meeting", args: { meetingId: "circuit-review", title: "Renamed Planning" } },
      { command: "export_meeting_json", args: { meetingId: "circuit-review" } },
      { command: "delete_meeting", args: { meetingId: "circuit-review" } },
      { command: "generate_summary", args: { meetingId: "circuit-review" } },
      { command: "cancel_summary", args: { jobId: "summary-circuit-review-1700000001000" } },
      { command: "save_whisper_model_path", args: { whisperModelPath: "/models/base.en.bin" } },
      {
        command: "save_analysis_settings",
        args: { ollamaBaseUrl: "http://127.0.0.1:11434", ollamaModel: "qwen3.6:27b" },
      },
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
});
