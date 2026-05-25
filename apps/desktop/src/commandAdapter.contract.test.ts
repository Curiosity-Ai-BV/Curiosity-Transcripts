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
});
