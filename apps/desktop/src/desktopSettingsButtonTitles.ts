type SettingsButtonTitleInput = {
  commandSurfaceReady: boolean;
  commandUnavailableTitle: string;
  commandBusy: boolean;
  busyCommandTitle: string;
};

type SettingsButtonTitles = {
  chooseWhisperModelButtonTitle: string;
  testWhisperButtonTitle: string;
  saveWhisperButtonTitle: string;
  testOllamaButtonTitle: string;
  saveAnalysisButtonTitle: string;
  saveRetentionButtonTitle: string;
};

export function deriveSettingsButtonTitles({
  commandSurfaceReady,
  commandUnavailableTitle,
  commandBusy,
  busyCommandTitle,
}: SettingsButtonTitleInput): SettingsButtonTitles {
  if (!commandSurfaceReady) {
    return {
      chooseWhisperModelButtonTitle: commandUnavailableTitle,
      testWhisperButtonTitle: commandUnavailableTitle,
      saveWhisperButtonTitle: commandUnavailableTitle,
      testOllamaButtonTitle: commandUnavailableTitle,
      saveAnalysisButtonTitle: commandUnavailableTitle,
      saveRetentionButtonTitle: commandUnavailableTitle,
    };
  }

  return {
    chooseWhisperModelButtonTitle: commandBusy
      ? busyCommandTitle
      : "Choose a local Whisper model file.",
    testWhisperButtonTitle: "Test the configured Whisper path.",
    saveWhisperButtonTitle: "Save the configured Whisper path.",
    testOllamaButtonTitle: "Test the configured local Ollama server and model.",
    saveAnalysisButtonTitle: "Save local analysis settings.",
    saveRetentionButtonTitle: "Save default raw-audio retention.",
  };
}
