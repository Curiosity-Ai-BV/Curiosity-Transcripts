import { describe, expect, it } from "vitest";

import { deriveTranscriptionButtonTitles } from "./desktopTranscriptionButtonTitles";

const commandUnavailableTitle = "Preview shell: backend command wiring is not connected in this browser/dev fixture.";
const busyCommandTitle = "A desktop command is already running.";
const readyTranscribeTitle = "Transcribe the selected meeting with the configured local Whisper model.";

function titles(overrides: Partial<Parameters<typeof deriveTranscriptionButtonTitles>[0]> = {}) {
  return deriveTranscriptionButtonTitles({
    commandSurfaceReady: true,
    commandUnavailableTitle,
    commandBusy: false,
    busyCommandTitle,
    selectedMeeting: {},
    modelKind: "ready",
    whisperModelReady: true,
    ...overrides,
  });
}

describe("transcription button title derivation", () => {
  it("uses command unavailable title before busy, selected meeting, model, or retry-ready state", () => {
    expect(
      titles({
        commandSurfaceReady: false,
        commandBusy: true,
        selectedMeeting: null,
        modelKind: "missing",
        whisperModelReady: false,
      }),
    ).toEqual({
      transcribeButtonTitle: commandUnavailableTitle,
      retryTranscriptionButtonTitle: commandUnavailableTitle,
    });
  });

  it("uses busy title before selected meeting, model, or retry-ready state", () => {
    expect(
      titles({
        commandBusy: true,
        selectedMeeting: null,
        modelKind: "missing",
        whisperModelReady: false,
      }),
    ).toEqual({
      transcribeButtonTitle: busyCommandTitle,
      retryTranscriptionButtonTitle: busyCommandTitle,
    });
  });

  it("uses no-selection guidance before model state for the transcribe title", () => {
    expect(
      titles({
        selectedMeeting: null,
        modelKind: "missing",
        whisperModelReady: true,
      }).transcribeButtonTitle,
    ).toBe("Select a meeting before transcription.");
  });

  it.each([
    ["missing", "Choose a local Whisper model file before transcription."],
    ["unsupported", "Choose a supported .bin or .gguf Whisper model file before transcription."],
    ["untested", "Run Test path for the saved Whisper model file before transcription."],
  ] as const)("uses the %s model block guidance for the transcribe title", (modelKind, expectedTitle) => {
    expect(titles({ modelKind, whisperModelReady: false }).transcribeButtonTitle).toBe(expectedTitle);
  });

  it("uses ready transcribe guidance for ready and other unblocked model kinds", () => {
    expect(titles({ modelKind: "ready", whisperModelReady: true }).transcribeButtonTitle).toBe(
      readyTranscribeTitle,
    );
    expect(titles({ modelKind: "transcribing", whisperModelReady: false }).transcribeButtonTitle).toBe(
      readyTranscribeTitle,
    );
  });

  it("uses the retry title only when the Whisper model is ready", () => {
    expect(titles({ whisperModelReady: true }).retryTranscriptionButtonTitle).toBe(
      "Retry transcription for the selected meeting.",
    );
  });

  it("reuses the transcribe title for retry when the Whisper model is not ready", () => {
    expect(
      titles({
        selectedMeeting: null,
        modelKind: "missing",
        whisperModelReady: false,
      }),
    ).toEqual({
      transcribeButtonTitle: "Select a meeting before transcription.",
      retryTranscriptionButtonTitle: "Select a meeting before transcription.",
    });
  });
});
