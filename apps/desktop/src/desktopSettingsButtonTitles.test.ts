import { describe, expect, it } from "vitest";

import { deriveSettingsButtonTitles } from "./desktopSettingsButtonTitles";

const commandUnavailableTitle = "Preview shell: backend command wiring is not connected in this browser/dev fixture.";
const busyCommandTitle = "A desktop command is already running.";

function titles(overrides: Partial<Parameters<typeof deriveSettingsButtonTitles>[0]> = {}) {
  return deriveSettingsButtonTitles({
    commandSurfaceReady: true,
    commandUnavailableTitle,
    commandBusy: false,
    busyCommandTitle,
    ...overrides,
  });
}

describe("settings button title derivation", () => {
  it("uses command unavailable title before busy state for every settings form action", () => {
    expect(
      titles({
        commandSurfaceReady: false,
        commandBusy: true,
      }),
    ).toEqual({
      chooseWhisperModelButtonTitle: commandUnavailableTitle,
      testWhisperButtonTitle: commandUnavailableTitle,
      saveWhisperButtonTitle: commandUnavailableTitle,
      testOllamaButtonTitle: commandUnavailableTitle,
      saveAnalysisButtonTitle: commandUnavailableTitle,
      saveRetentionButtonTitle: commandUnavailableTitle,
    });
  });

  it("uses busy title only for choosing the Whisper model when commands are ready", () => {
    expect(titles({ commandBusy: true })).toEqual({
      chooseWhisperModelButtonTitle: busyCommandTitle,
      testWhisperButtonTitle: "Test the configured Whisper path.",
      saveWhisperButtonTitle: "Save the configured Whisper path.",
      testOllamaButtonTitle: "Test the configured local Ollama server and model.",
      saveAnalysisButtonTitle: "Save local analysis settings.",
      saveRetentionButtonTitle: "Save default raw-audio retention.",
    });
  });

  it("uses ready titles when command surface is ready and idle", () => {
    expect(titles()).toEqual({
      chooseWhisperModelButtonTitle: "Choose a local Whisper model file.",
      testWhisperButtonTitle: "Test the configured Whisper path.",
      saveWhisperButtonTitle: "Save the configured Whisper path.",
      testOllamaButtonTitle: "Test the configured local Ollama server and model.",
      saveAnalysisButtonTitle: "Save local analysis settings.",
      saveRetentionButtonTitle: "Save default raw-audio retention.",
    });
  });
});
