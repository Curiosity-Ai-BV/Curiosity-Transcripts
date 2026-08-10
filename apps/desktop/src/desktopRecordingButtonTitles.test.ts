import { describe, expect, it } from "vitest";

import { deriveRecordingButtonTitles } from "./desktopRecordingButtonTitles";

const commandUnavailableTitle = "Preview shell: backend command wiring is not connected in this browser/dev fixture.";
const busyCommandTitle = "A desktop command is already running.";

function titles(overrides: Partial<Parameters<typeof deriveRecordingButtonTitles>[0]> = {}) {
  return deriveRecordingButtonTitles({
    commandSurfaceReady: true,
    commandUnavailableTitle,
    commandBusy: false,
    busyCommandTitle,
    isRecordingActive: false,
    importWavPath: "",
    ...overrides,
  });
}

describe("recording button title derivation", () => {
  it("uses command unavailable title before busy, active recording, or import path state", () => {
    expect(
      titles({
        commandSurfaceReady: false,
        commandBusy: true,
        isRecordingActive: true,
        importWavPath: "/tmp/customer-call.wav",
      }),
    ).toEqual({
      startButtonTitle: commandUnavailableTitle,
      stopButtonTitle: commandUnavailableTitle,
      importButtonTitle: commandUnavailableTitle,
      chooseWavButtonTitle: commandUnavailableTitle,
    });
  });

  it("uses busy title before active recording or import path state", () => {
    expect(
      titles({
        commandBusy: true,
        isRecordingActive: true,
        importWavPath: "/tmp/customer-call.wav",
      }),
    ).toEqual({
      startButtonTitle: busyCommandTitle,
      stopButtonTitle: busyCommandTitle,
      importButtonTitle: busyCommandTitle,
      chooseWavButtonTitle: busyCommandTitle,
    });
  });

  it("keeps active recording messages for start, stop, import, and choose controls", () => {
    expect(titles({ isRecordingActive: true })).toEqual({
      startButtonTitle: "Stop the active recording before starting another one.",
      stopButtonTitle: "Stop desktop recording.",
      importButtonTitle: "Stop the active recording before importing audio.",
      chooseWavButtonTitle: "Stop the active recording before choosing audio.",
    });
  });

  it("uses the trimmed WAV path when choosing the import title", () => {
    expect(titles({ importWavPath: "   " }).importButtonTitle).toBe(
      "Enter a local WAV source path before importing.",
    );
    expect(titles({ importWavPath: "  /tmp/customer-call.wav  " }).importButtonTitle).toBe(
      "Import the WAV file into private app storage.",
    );
  });

  it("uses default ready titles when no command is busy and no recording is active", () => {
    expect(titles()).toEqual({
      startButtonTitle: "Start desktop recording.",
      stopButtonTitle: "No active desktop recording to stop.",
      importButtonTitle: "Enter a local WAV source path before importing.",
      chooseWavButtonTitle: "Choose a local WAV source file.",
    });
  });
});
