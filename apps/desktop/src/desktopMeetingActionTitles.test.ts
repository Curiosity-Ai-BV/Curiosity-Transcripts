import { describe, expect, it } from "vitest";

import { deriveMeetingActionTitles } from "./desktopMeetingActionTitles";

const commandUnavailableTitle = "Preview shell: backend command wiring is not connected in this browser/dev fixture.";
const busyCommandTitle = "A desktop command is already running.";

function titles(overrides: Partial<Parameters<typeof deriveMeetingActionTitles>[0]> = {}) {
  return deriveMeetingActionTitles({
    commandSurfaceReady: true,
    commandUnavailableTitle,
    commandBusy: false,
    busyCommandTitle,
    selectedMeeting: { segments: [{ id: "segment-1" }] },
    selectedExportFormatLabel: "JSON",
    selectedMeetingHasActiveDeleteBlockingJob: false,
    ollamaSummaryBlockGuidance: null,
    ...overrides,
  });
}

describe("meeting action title derivation", () => {
  it("uses command unavailable title before busy, selected meeting, delete job, or summary block state", () => {
    expect(
      titles({
        commandSurfaceReady: false,
        commandBusy: true,
        selectedMeetingHasActiveDeleteBlockingJob: true,
        ollamaSummaryBlockGuidance: "Run Test Ollama before requesting a summary.",
      }),
    ).toEqual({
      renameButtonTitle: commandUnavailableTitle,
      exportButtonTitle: commandUnavailableTitle,
      deleteButtonTitle: commandUnavailableTitle,
      summaryButtonTitle: commandUnavailableTitle,
    });
  });

  it("uses busy title before selected meeting, delete job, empty transcript, or Ollama block state", () => {
    expect(
      titles({
        commandBusy: true,
        selectedMeeting: { segments: [] },
        selectedMeetingHasActiveDeleteBlockingJob: true,
        ollamaSummaryBlockGuidance: "Run Test Ollama before requesting a summary.",
      }),
    ).toEqual({
      renameButtonTitle: busyCommandTitle,
      exportButtonTitle: busyCommandTitle,
      deleteButtonTitle: busyCommandTitle,
      summaryButtonTitle: busyCommandTitle,
    });
  });

  it("uses no-selection guidance for meeting actions that require a selected meeting", () => {
    expect(titles({ selectedMeeting: null })).toEqual({
      renameButtonTitle: "Select a meeting before renaming.",
      exportButtonTitle: "Select a meeting before exporting.",
      deleteButtonTitle: "Select a meeting before deleting private data.",
      summaryButtonTitle: "Select a meeting before requesting a summary.",
    });
  });

  it("uses selected meeting ready titles and the supplied export label", () => {
    expect(titles({ selectedExportFormatLabel: "Markdown" })).toEqual({
      renameButtonTitle: "Rename the selected meeting.",
      exportButtonTitle: "Export the selected meeting as Markdown.",
      deleteButtonTitle: "Delete app-private data for the selected meeting.",
      summaryButtonTitle: "Generate a local Ollama summary for the selected meeting.",
    });
  });

  it("uses the active-job delete block before the selected-meeting delete title", () => {
    expect(titles({ selectedMeetingHasActiveDeleteBlockingJob: true }).deleteButtonTitle).toBe(
      "Cancel or wait for the active transcription or summary job before deleting private data.",
    );
  });

  it("uses empty-transcript guidance before Ollama summary block guidance", () => {
    expect(
      titles({
        selectedMeeting: { segments: [] },
        ollamaSummaryBlockGuidance: "Run Test Ollama before requesting a summary.",
      }).summaryButtonTitle,
    ).toBe("Generate a transcript before requesting a summary.");
  });

  it("uses Ollama block guidance before the summary ready title", () => {
    expect(
      titles({
        ollamaSummaryBlockGuidance:
          "Run `ollama pull qwen3.6:27b`, then run Test Ollama again. Availability is not checked in the background.",
      }).summaryButtonTitle,
    ).toBe(
      "Run `ollama pull qwen3.6:27b`, then run Test Ollama again. Availability is not checked in the background.",
    );
  });
});
