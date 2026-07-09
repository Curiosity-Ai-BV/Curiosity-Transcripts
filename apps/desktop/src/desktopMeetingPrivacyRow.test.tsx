import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import type { StatusView } from "./commandAdapter";
import { MeetingPrivacyRow, type MeetingPrivacyRowProps } from "./desktopMeetingPrivacyRow";

function statusView(overrides: Partial<StatusView> = {}): StatusView {
  return {
    label: "Available",
    detail: "Ready to use.",
    tone: "ready",
    ...overrides,
  };
}

function meetingPrivacyRowProps(overrides: Partial<MeetingPrivacyRowProps> = {}): MeetingPrivacyRowProps {
  return {
    storage: {
      label: "Private storage",
      path: "Application data/meetings/circuit-review",
    },
    rawAudioRetention: null,
    localProcessing: null,
    calendarContext: null,
    exportState: statusView({
      label: "Export",
      detail: "Ready to export.",
      tone: "active",
    }),
    deleteState: statusView({
      label: "Delete",
      detail: "No deletion queued.",
      tone: "muted",
    }),
    ...overrides,
  };
}

function statusLineFor(label: string): Element | null {
  return screen.getByText(label).closest(".status-line");
}

afterEach(() => {
  cleanup();
});

describe("MeetingPrivacyRow", () => {
  it("renders storage, export, and delete status lines with their labels, values, and tones", () => {
    render(<MeetingPrivacyRow {...meetingPrivacyRowProps()} />);

    expect(screen.getByText("Private storage")).toBeInTheDocument();
    expect(screen.getByText("Application data/meetings/circuit-review")).toBeInTheDocument();
    expect(statusLineFor("Private storage")).toHaveClass("status-line", "ready");

    expect(screen.getByText("Export")).toBeInTheDocument();
    expect(screen.getByText("Ready to export.")).toBeInTheDocument();
    expect(statusLineFor("Export")).toHaveClass("status-line", "active");

    expect(screen.getByText("Delete")).toBeInTheDocument();
    expect(screen.getByText("No deletion queued.")).toBeInTheDocument();
    expect(statusLineFor("Delete")).toHaveClass("status-line", "muted");
  });

  it("renders raw-audio and local-processing status lines only when provided", () => {
    const { rerender } = render(<MeetingPrivacyRow {...meetingPrivacyRowProps()} />);

    expect(screen.queryByText("Raw audio")).not.toBeInTheDocument();
    expect(screen.queryByText("Local processing")).not.toBeInTheDocument();

    rerender(
      <MeetingPrivacyRow
        {...meetingPrivacyRowProps({
          rawAudioRetention: statusView({
            label: "Raw audio",
            detail: "Deleted after transcription.",
            tone: "warn",
          }),
          localProcessing: statusView({
            label: "Local processing",
            detail: "Processed on this device.",
            tone: "ready",
          }),
        })}
      />,
    );

    expect(screen.getByText("Deleted after transcription.")).toBeInTheDocument();
    expect(statusLineFor("Raw audio")).toHaveClass("status-line", "warn");
    expect(screen.getByText("Processed on this device.")).toBeInTheDocument();
    expect(statusLineFor("Local processing")).toHaveClass("status-line", "ready");
  });

  it("renders calendar context with the fixed label only when a formatted value is provided", () => {
    const { rerender } = render(<MeetingPrivacyRow {...meetingPrivacyRowProps()} />);

    expect(screen.queryByText("Calendar context")).not.toBeInTheDocument();

    rerender(
      <MeetingPrivacyRow
        {...meetingPrivacyRowProps({
          calendarContext: "Attached: Product sync, Jul 09, 2026.",
        })}
      />,
    );

    expect(screen.getByText("Calendar context")).toBeInTheDocument();
    expect(screen.getByText("Attached: Product sync, Jul 09, 2026.")).toBeInTheDocument();
    expect(statusLineFor("Calendar context")).toHaveClass("status-line", "ready");
  });

  it("renders calendar context when the formatted value is an empty string", () => {
    render(<MeetingPrivacyRow {...meetingPrivacyRowProps({ calendarContext: "" })} />);

    const calendarLine = statusLineFor("Calendar context");

    expect(calendarLine).toHaveClass("status-line", "ready");
    expect(calendarLine?.querySelector("small")?.textContent).toBe("");
  });

  it("keeps the root privacy row class and accessible label", () => {
    render(<MeetingPrivacyRow {...meetingPrivacyRowProps()} />);

    expect(screen.getByLabelText("Meeting privacy data state")).toHaveClass("privacy-row");
  });
});
