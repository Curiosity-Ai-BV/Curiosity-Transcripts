import type { ModelStatus } from "./desktopContract";

type TranscriptionButtonTitleInput = {
  commandSurfaceReady: boolean;
  commandUnavailableTitle: string;
  commandBusy: boolean;
  busyCommandTitle: string;
  selectedMeeting: unknown | null | undefined;
  modelKind: ModelStatus["kind"];
  whisperModelReady: boolean;
};

type TranscriptionButtonTitles = {
  transcribeButtonTitle: string;
  retryTranscriptionButtonTitle: string;
};

export function deriveTranscriptionButtonTitles({
  commandSurfaceReady,
  commandUnavailableTitle,
  commandBusy,
  busyCommandTitle,
  selectedMeeting,
  modelKind,
  whisperModelReady,
}: TranscriptionButtonTitleInput): TranscriptionButtonTitles {
  const transcribeButtonTitle = !commandSurfaceReady
    ? commandUnavailableTitle
    : commandBusy
      ? busyCommandTitle
      : !selectedMeeting
        ? "Select a meeting before transcription."
        : modelKind === "missing"
          ? "Choose a local Whisper model file before transcription."
          : modelKind === "unsupported"
            ? "Choose a supported .bin or .gguf Whisper model file before transcription."
          : modelKind === "untested"
            ? "Run Test path for the saved Whisper model file before transcription."
            : "Transcribe the selected meeting with the configured local Whisper model.";
  const retryTranscriptionButtonTitle = !commandSurfaceReady
    ? commandUnavailableTitle
    : commandBusy
      ? busyCommandTitle
      : !whisperModelReady
        ? transcribeButtonTitle
        : "Retry transcription for the selected meeting.";

  return { transcribeButtonTitle, retryTranscriptionButtonTitle };
}
