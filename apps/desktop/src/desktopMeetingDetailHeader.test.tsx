import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";

import type { MeetingView } from "./commandAdapter";
import { MeetingDetailHeader, type MeetingDetailHeaderProps } from "./desktopMeetingDetailHeader";

type MeetingDetailHeaderTestMeeting = Pick<MeetingView, "startedAt" | "title" | "transcriptState">;

function meeting(overrides: Partial<MeetingDetailHeaderTestMeeting> = {}): MeetingDetailHeaderTestMeeting {
  return {
    startedAt: "Jul 09, 2026",
    title: "Circuit Review",
    transcriptState: "Ready",
    ...overrides,
  };
}

function meetingDetailHeaderProps(overrides: Partial<MeetingDetailHeaderProps> = {}): MeetingDetailHeaderProps {
  return {
    meeting: meeting(),
    renameTitle: "Circuit Review",
    renameInputDisabled: false,
    renameDisabled: false,
    transcribeDisabled: false,
    renameButtonTitle: "Rename this meeting.",
    transcribeButtonTitle: "Transcribe this meeting.",
    pendingCommand: null,
    onRenameTitleChange: vi.fn(),
    onRename: vi.fn(),
    onTranscribe: vi.fn(),
    ...overrides,
  };
}

afterEach(() => {
  cleanup();
});

describe("MeetingDetailHeader", () => {
  it("renders meeting date/title, rename controls, transcript status, and action buttons", () => {
    const { container } = render(<MeetingDetailHeader {...meetingDetailHeaderProps()} />);

    expect(container.firstElementChild).toHaveClass("detail-header");
    expect(screen.getByText("Jul 09, 2026")).toHaveClass("eyebrow");
    expect(screen.getByRole("heading", { name: "Circuit Review" })).toBeInTheDocument();
    expect(screen.getByText("Selected meeting title")).toBeInTheDocument();
    expect(screen.getByLabelText("Selected meeting title")).toHaveValue("Circuit Review");
    expect(screen.getByText("Ready")).toHaveClass("status-pill");
    expect(screen.getByRole("button", { name: "Rename" })).toHaveClass("button");
    expect(screen.getByRole("button", { name: "Transcribe" })).toHaveClass("button", "primary");
  });

  it("propagates rename input changes through onRenameTitleChange", () => {
    const onRenameTitleChange = vi.fn();

    render(<MeetingDetailHeader {...meetingDetailHeaderProps({ onRenameTitleChange })} />);

    fireEvent.change(screen.getByLabelText("Selected meeting title"), { target: { value: "Updated Review" } });

    expect(onRenameTitleChange).toHaveBeenCalledWith("Updated Review");
  });

  it("calls rename and transcribe callbacks from the right buttons", async () => {
    const user = userEvent.setup();
    const onRename = vi.fn();
    const onTranscribe = vi.fn();

    render(<MeetingDetailHeader {...meetingDetailHeaderProps({ onRename, onTranscribe })} />);

    await user.click(screen.getByRole("button", { name: "Rename" }));
    await user.click(screen.getByRole("button", { name: "Transcribe" }));

    expect(onRename).toHaveBeenCalledTimes(1);
    expect(onTranscribe).toHaveBeenCalledTimes(1);
  });

  it("applies disabled and title props to the rename input/button and transcribe button", () => {
    render(
      <MeetingDetailHeader
        {...meetingDetailHeaderProps({
          renameInputDisabled: true,
          renameDisabled: true,
          transcribeDisabled: true,
          renameButtonTitle: "Rename disabled",
          transcribeButtonTitle: "Transcribe disabled",
        })}
      />,
    );

    expect(screen.getByLabelText("Selected meeting title")).toBeDisabled();
    expect(screen.getByRole("button", { name: "Rename" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Rename" })).toHaveAttribute("title", "Rename disabled");
    expect(screen.getByRole("button", { name: "Transcribe" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Transcribe" })).toHaveAttribute("title", "Transcribe disabled");
  });

  it("renders pending labels for rename and transcribe states", () => {
    const { rerender } = render(<MeetingDetailHeader {...meetingDetailHeaderProps({ pendingCommand: "rename" })} />);

    expect(screen.getByRole("button", { name: "Renaming" })).toBeInTheDocument();

    rerender(<MeetingDetailHeader {...meetingDetailHeaderProps({ pendingCommand: "transcribe" })} />);
    expect(screen.getByRole("button", { name: "Transcribing" })).toBeInTheDocument();
  });

  it("uses ready pill tone for Ready and active pill tone for non-Ready transcript states", () => {
    const { rerender } = render(<MeetingDetailHeader {...meetingDetailHeaderProps()} />);

    expect(screen.getByText("Ready")).toHaveClass("status-pill", "ready");

    rerender(<MeetingDetailHeader {...meetingDetailHeaderProps({ meeting: meeting({ transcriptState: "Transcribing" }) })} />);
    expect(screen.getByText("Transcribing")).toHaveClass("status-pill", "active");

    rerender(<MeetingDetailHeader {...meetingDetailHeaderProps({ meeting: meeting({ transcriptState: "Unavailable" }) })} />);
    expect(screen.getByText("Unavailable")).toHaveClass("status-pill", "active");
  });
});
