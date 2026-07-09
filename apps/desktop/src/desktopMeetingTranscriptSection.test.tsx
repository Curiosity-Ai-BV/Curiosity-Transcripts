import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";

import type { TranscriptSegment } from "./commandAdapter";
import { MeetingTranscriptSection, type MeetingTranscriptSectionProps } from "./desktopMeetingTranscriptSection";

function transcriptSegment(overrides: Partial<TranscriptSegment> = {}): TranscriptSegment {
  return {
    id: "segment-1",
    startMs: 65_000,
    endMs: 72_000,
    text: "We decided to keep raw audio retention visible.",
    originalText: null,
    sourceChannel: "microphone",
    modelRunId: "model-run-1",
    transcriptVersionId: "version-1",
    ...overrides,
  };
}

function meetingTranscriptSectionProps(
  overrides: Partial<MeetingTranscriptSectionProps> = {},
): MeetingTranscriptSectionProps {
  return {
    segments: [
      transcriptSegment(),
      transcriptSegment({
        id: "segment-2",
        startMs: 130_000,
        endMs: 139_000,
        text: "Exports should show when files remain outside app control.",
        sourceChannel: "system",
      }),
    ],
    editingSegmentId: null,
    segmentDraft: "",
    segmentDraftDisabled: false,
    correctionDisabled: false,
    saveCorrectionTitle: "Save the user correction for this transcript segment.",
    cancelCorrectionDisabled: false,
    editSegmentDisabled: false,
    editSegmentTitle: "Edit this transcript segment.",
    pendingCommand: null,
    onSegmentDraftChange: vi.fn(),
    onEditSegment: vi.fn(),
    onCancelCorrection: vi.fn(),
    onSaveCorrection: vi.fn(),
    ...overrides,
  };
}

afterEach(() => {
  cleanup();
});

describe("MeetingTranscriptSection", () => {
  it("renders the transcript heading, segment text, formatted time, source channel, and edit button", () => {
    const { container } = render(<MeetingTranscriptSection {...meetingTranscriptSectionProps()} />);
    const firstSegment = screen
      .getByText("We decided to keep raw audio retention visible.")
      .closest("article");

    expect(container.firstElementChild).toHaveClass("transcript-section");
    expect(screen.getByRole("heading", { name: "Transcript" })).toBeInTheDocument();
    expect(container.querySelector(".segments")).toBeInTheDocument();
    expect(firstSegment).toHaveClass("segment");
    expect(within(firstSegment!).getByText("1:05")).toBeInTheDocument();
    expect(within(firstSegment!).getByText("microphone")).toHaveClass("segment-channel");
    expect(within(firstSegment!).getByRole("button", { name: "Edit segment" })).toHaveClass(
      "button",
      "quiet",
      "segment-edit-button",
    );
  });

  it("renders original text only when it exists and differs from the current text", () => {
    render(
      <MeetingTranscriptSection
        {...meetingTranscriptSectionProps({
          segments: [
            transcriptSegment({
              id: "segment-1",
              text: "Corrected retention decision.",
              originalText: "Original retention decision.",
            }),
            transcriptSegment({
              id: "segment-2",
              text: "Unchanged transcript text.",
              originalText: "Unchanged transcript text.",
            }),
            transcriptSegment({
              id: "segment-3",
              text: "Transcript without original text.",
              originalText: null,
            }),
          ],
        })}
      />,
    );

    expect(screen.getByText("Original: Original retention decision.")).toHaveClass("segment-original");
    expect(screen.queryByText("Original: Unchanged transcript text.")).not.toBeInTheDocument();
    expect(screen.queryByText("Original:")).not.toBeInTheDocument();
  });

  it("calls onEditSegment with the segment id and current text from the right edit button", async () => {
    const user = userEvent.setup();
    const onEditSegment = vi.fn();

    render(<MeetingTranscriptSection {...meetingTranscriptSectionProps({ onEditSegment })} />);

    const secondSegment = screen
      .getByText("Exports should show when files remain outside app control.")
      .closest("article");
    await user.click(within(secondSegment!).getByRole("button", { name: "Edit segment" }));

    expect(onEditSegment).toHaveBeenCalledWith(
      "segment-2",
      "Exports should show when files remain outside app control.",
    );
  });

  it("renders edit mode for the selected segment with a controlled textarea and propagates draft changes", () => {
    const onSegmentDraftChange = vi.fn();

    render(
      <MeetingTranscriptSection
        {...meetingTranscriptSectionProps({
          editingSegmentId: "segment-2",
          segmentDraft: "Updated segment draft.",
          onSegmentDraftChange,
        })}
      />,
    );

    const editor = screen.getByLabelText("Transcript segment text");

    expect(screen.getAllByLabelText("Transcript segment text")).toHaveLength(1);
    expect(editor).toHaveValue("Updated segment draft.");
    expect(editor.closest(".segment-editor")).toBeInTheDocument();
    expect(editor.closest("label")).toHaveClass("segment-editor-field");
    fireEvent.change(editor, { target: { value: "Next segment draft." } });
    expect(onSegmentDraftChange).toHaveBeenCalledWith("Next segment draft.");
  });

  it("calls save and cancel callbacks and applies correction disabled and ready title behavior", async () => {
    const user = userEvent.setup();
    const onSaveCorrection = vi.fn();
    const onCancelCorrection = vi.fn();
    const { rerender } = render(
      <MeetingTranscriptSection
        {...meetingTranscriptSectionProps({
          editingSegmentId: "segment-1",
          segmentDraft: "Corrected transcript text.",
          onSaveCorrection,
          onCancelCorrection,
        })}
      />,
    );

    const saveButton = screen.getByRole("button", { name: "Save correction" });

    expect(saveButton).toHaveClass("button", "primary");
    expect(saveButton).toHaveAttribute("title", "Save the user correction for this transcript segment.");
    await user.click(saveButton);
    await user.click(screen.getByRole("button", { name: "Cancel correction" }));

    expect(onSaveCorrection).toHaveBeenCalledTimes(1);
    expect(onCancelCorrection).toHaveBeenCalledTimes(1);

    rerender(
      <MeetingTranscriptSection
        {...meetingTranscriptSectionProps({
          editingSegmentId: "segment-1",
          segmentDraft: "",
          correctionDisabled: true,
        })}
      />,
    );
    expect(screen.getByRole("button", { name: "Save correction" })).toBeDisabled();
  });

  it("renders pending correction labels and disables the textarea and cancel action", () => {
    render(
      <MeetingTranscriptSection
        {...meetingTranscriptSectionProps({
          editingSegmentId: "segment-1",
          segmentDraft: "Correction in flight.",
          segmentDraftDisabled: true,
          correctionDisabled: true,
          cancelCorrectionDisabled: true,
          pendingCommand: "correct-segment",
        })}
      />,
    );

    expect(screen.getByLabelText("Transcript segment text")).toBeDisabled();
    expect(screen.getByRole("button", { name: "Saving correction" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Cancel correction" })).toBeDisabled();
  });

  it("uses the unavailable title and disables edit and save affordances when commands are unavailable", () => {
    const { rerender } = render(
      <MeetingTranscriptSection
        {...meetingTranscriptSectionProps({
          editSegmentDisabled: true,
          editSegmentTitle: "Connect the desktop command surface first.",
        })}
      />,
    );

    expect(screen.getAllByRole("button", { name: "Edit segment" })[0]).toBeDisabled();
    expect(screen.getAllByRole("button", { name: "Edit segment" })[0]).toHaveAttribute(
      "title",
      "Connect the desktop command surface first.",
    );

    rerender(
      <MeetingTranscriptSection
        {...meetingTranscriptSectionProps({
          editingSegmentId: "segment-1",
          segmentDraft: "Unavailable correction.",
          correctionDisabled: true,
          saveCorrectionTitle: "Connect the desktop command surface first.",
        })}
      />,
    );
    expect(screen.getByRole("button", { name: "Save correction" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Save correction" })).toHaveAttribute(
      "title",
      "Connect the desktop command surface first.",
    );
  });
});
