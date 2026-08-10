type RecordingButtonTitleInput = {
  commandSurfaceReady: boolean;
  commandUnavailableTitle: string;
  commandBusy: boolean;
  busyCommandTitle: string;
  isRecordingActive: boolean;
  importWavPath: string;
};

type RecordingButtonTitles = {
  startButtonTitle: string;
  stopButtonTitle: string;
  importButtonTitle: string;
  chooseWavButtonTitle: string;
};

export function deriveRecordingButtonTitles({
  commandSurfaceReady,
  commandUnavailableTitle,
  commandBusy,
  busyCommandTitle,
  isRecordingActive,
  importWavPath,
}: RecordingButtonTitleInput): RecordingButtonTitles {
  if (!commandSurfaceReady) {
    return {
      startButtonTitle: commandUnavailableTitle,
      stopButtonTitle: commandUnavailableTitle,
      importButtonTitle: commandUnavailableTitle,
      chooseWavButtonTitle: commandUnavailableTitle,
    };
  }

  if (commandBusy) {
    return {
      startButtonTitle: busyCommandTitle,
      stopButtonTitle: busyCommandTitle,
      importButtonTitle: busyCommandTitle,
      chooseWavButtonTitle: busyCommandTitle,
    };
  }

  return {
    startButtonTitle: isRecordingActive
      ? "Stop the active recording before starting another one."
      : "Start desktop recording.",
    stopButtonTitle: isRecordingActive
      ? "Stop desktop recording."
      : "No active desktop recording to stop.",
    importButtonTitle: isRecordingActive
      ? "Stop the active recording before importing audio."
      : importWavPath.trim()
        ? "Import the WAV file into private app storage."
        : "Enter a local WAV source path before importing.",
    chooseWavButtonTitle: isRecordingActive
      ? "Stop the active recording before choosing audio."
      : "Choose a local WAV source file.",
  };
}
