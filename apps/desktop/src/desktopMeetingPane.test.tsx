import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";

import type { MeetingView } from "./commandAdapter";
import { MeetingPane } from "./desktopMeetingPane";

type MeetingPaneTestMeeting = Pick<MeetingView, "id" | "title" | "startedAt" | "duration" | "transcriptState">;

function meetingView(overrides: Partial<MeetingPaneTestMeeting> & Pick<MeetingPaneTestMeeting, "id" | "title">): MeetingPaneTestMeeting {
  const { id, title, ...rest } = overrides;
  return {
    id,
    title,
    startedAt: "Jul 08, 2026",
    duration: "42 min",
    transcriptState: "Ready",
    ...rest,
  };
}

const meetings = [
  meetingView({ id: "circuit-review", title: "Circuit Review" }),
  meetingView({
    id: "design-standup",
    title: "Design Standup",
    startedAt: "Jul 09, 2026",
    duration: "18 min",
    transcriptState: "Transcribing",
  }),
];

afterEach(() => {
  cleanup();
});

describe("MeetingPane", () => {
  it("renders the search heading, label, and meeting rows with metadata and state", () => {
    render(
      <MeetingPane
        query=""
        meetings={meetings}
        selectedMeetingId={null}
        loading={false}
        onQueryChange={vi.fn()}
        onSelectMeeting={vi.fn()}
      />,
    );

    expect(screen.getByLabelText("Meetings")).toHaveClass("meeting-pane");
    expect(screen.getByText("History")).toHaveClass("eyebrow");
    expect(screen.getByRole("heading", { name: "Meetings" })).toBeInTheDocument();
    expect(screen.getByLabelText("Search meetings")).toHaveAttribute("placeholder", "Title or transcript text");

    const firstMeeting = screen.getByRole("button", { name: /Circuit Review/ });
    expect(within(firstMeeting).getByText("Circuit Review")).toHaveClass("meeting-title");
    expect(within(firstMeeting).getByText("Jul 08, 2026 / 42 min")).toHaveClass("meeting-meta");
    expect(within(firstMeeting).getByText("Ready")).toHaveClass("meeting-state");

    const secondMeeting = screen.getByRole("button", { name: /Design Standup/ });
    expect(within(secondMeeting).getByText("Jul 09, 2026 / 18 min")).toHaveClass("meeting-meta");
    expect(within(secondMeeting).getByText("Transcribing")).toHaveClass("meeting-state");
  });

  it("marks the selected meeting for visual and assistive selection state", () => {
    render(
      <MeetingPane
        query=""
        meetings={meetings}
        selectedMeetingId="design-standup"
        loading={false}
        onQueryChange={vi.fn()}
        onSelectMeeting={vi.fn()}
      />,
    );

    const selectedMeeting = screen.getByRole("button", { name: /Design Standup/ });
    expect(selectedMeeting).toHaveClass("meeting-row", "selected");
    expect(selectedMeeting).toHaveAttribute("aria-pressed", "true");
    expect(selectedMeeting).toHaveAttribute("aria-current", "page");

    const otherMeeting = screen.getByRole("button", { name: /Circuit Review/ });
    expect(otherMeeting).toHaveClass("meeting-row");
    expect(otherMeeting).not.toHaveClass("selected");
    expect(otherMeeting).toHaveAttribute("aria-pressed", "false");
    expect(otherMeeting).not.toHaveAttribute("aria-current");
  });

  it("propagates search input changes through onQueryChange", () => {
    const onQueryChange = vi.fn();

    render(
      <MeetingPane
        query=""
        meetings={meetings}
        selectedMeetingId={null}
        loading={false}
        onQueryChange={onQueryChange}
        onSelectMeeting={vi.fn()}
      />,
    );

    fireEvent.change(screen.getByLabelText("Search meetings"), { target: { value: "design" } });

    expect(onQueryChange).toHaveBeenLastCalledWith("design");
  });

  it("calls onSelectMeeting with the clicked meeting id", async () => {
    const user = userEvent.setup();
    const onSelectMeeting = vi.fn();

    render(
      <MeetingPane
        query=""
        meetings={meetings}
        selectedMeetingId={null}
        loading={false}
        onQueryChange={vi.fn()}
        onSelectMeeting={onSelectMeeting}
      />,
    );

    await user.click(screen.getByRole("button", { name: /Design Standup/ }));

    expect(onSelectMeeting).toHaveBeenCalledTimes(1);
    expect(onSelectMeeting).toHaveBeenCalledWith("design-standup");
  });

  it("renders the loading skeleton when loading is true", () => {
    render(
      <MeetingPane
        query=""
        meetings={meetings}
        selectedMeetingId={null}
        loading={true}
        onQueryChange={vi.fn()}
        onSelectMeeting={vi.fn()}
      />,
    );

    expect(screen.getByLabelText("Loading workspace")).toHaveClass("skeleton-list");
  });

  it("renders the empty search message only for a non-loading non-empty query with no meetings", () => {
    const { rerender } = render(
      <MeetingPane
        query="missing"
        meetings={[]}
        selectedMeetingId={null}
        loading={false}
        onQueryChange={vi.fn()}
        onSelectMeeting={vi.fn()}
      />,
    );

    expect(screen.getByText("No meetings match this search.")).toHaveClass("empty-state");

    rerender(
      <MeetingPane
        query="missing"
        meetings={[]}
        selectedMeetingId={null}
        loading={true}
        onQueryChange={vi.fn()}
        onSelectMeeting={vi.fn()}
      />,
    );
    expect(screen.queryByText("No meetings match this search.")).not.toBeInTheDocument();

    rerender(
      <MeetingPane
        query=""
        meetings={[]}
        selectedMeetingId={null}
        loading={false}
        onQueryChange={vi.fn()}
        onSelectMeeting={vi.fn()}
      />,
    );
    expect(screen.queryByText("No meetings match this search.")).not.toBeInTheDocument();
  });
});
