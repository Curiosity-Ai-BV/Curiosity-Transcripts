import { afterEach, describe, expect, it, vi } from "vitest";

import {
  CommandFetcher,
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
