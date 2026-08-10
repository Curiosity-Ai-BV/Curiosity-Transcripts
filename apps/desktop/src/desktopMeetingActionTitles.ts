type MeetingActionTitleMeeting = {
  segments: readonly unknown[];
};

type MeetingActionTitleInput = {
  commandSurfaceReady: boolean;
  commandUnavailableTitle: string;
  commandBusy: boolean;
  busyCommandTitle: string;
  selectedMeeting: MeetingActionTitleMeeting | null | undefined;
  selectedExportFormatLabel: string;
  selectedMeetingHasActiveDeleteBlockingJob: boolean;
  ollamaSummaryBlockGuidance: string | null | undefined;
};

type MeetingActionTitles = {
  renameButtonTitle: string;
  exportButtonTitle: string;
  deleteButtonTitle: string;
  summaryButtonTitle: string;
};

export function deriveMeetingActionTitles({
  commandSurfaceReady,
  commandUnavailableTitle,
  commandBusy,
  busyCommandTitle,
  selectedMeeting,
  selectedExportFormatLabel,
  selectedMeetingHasActiveDeleteBlockingJob,
  ollamaSummaryBlockGuidance,
}: MeetingActionTitleInput): MeetingActionTitles {
  if (!commandSurfaceReady) {
    return {
      renameButtonTitle: commandUnavailableTitle,
      exportButtonTitle: commandUnavailableTitle,
      deleteButtonTitle: commandUnavailableTitle,
      summaryButtonTitle: commandUnavailableTitle,
    };
  }

  if (commandBusy) {
    return {
      renameButtonTitle: busyCommandTitle,
      exportButtonTitle: busyCommandTitle,
      deleteButtonTitle: busyCommandTitle,
      summaryButtonTitle: busyCommandTitle,
    };
  }

  return {
    renameButtonTitle: selectedMeeting ? "Rename the selected meeting." : "Select a meeting before renaming.",
    exportButtonTitle: selectedMeeting
      ? `Export the selected meeting as ${selectedExportFormatLabel}.`
      : "Select a meeting before exporting.",
    deleteButtonTitle: selectedMeetingHasActiveDeleteBlockingJob
      ? "Cancel or wait for the active transcription or summary job before deleting private data."
      : selectedMeeting
        ? "Delete app-private data for the selected meeting."
        : "Select a meeting before deleting private data.",
    summaryButtonTitle: !selectedMeeting
      ? "Select a meeting before requesting a summary."
      : selectedMeeting.segments.length === 0
        ? "Generate a transcript before requesting a summary."
        : ollamaSummaryBlockGuidance
          ? ollamaSummaryBlockGuidance
          : "Generate a local Ollama summary for the selected meeting.",
  };
}
