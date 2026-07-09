import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";

import type { StatusView } from "./commandAdapter";
import { DesktopCommandOutcomes, type DesktopCommandOutcomesProps } from "./desktopCommandOutcomes";

function statusView(overrides: Partial<StatusView> = {}): StatusView {
  return {
    label: "Command complete",
    detail: "The desktop command finished.",
    tone: "ready",
    ...overrides,
  };
}

function desktopCommandOutcomesProps(
  overrides: Partial<DesktopCommandOutcomesProps> = {},
): DesktopCommandOutcomesProps {
  return {
    commandError: null,
    showExportOutcome: false,
    exportCommandState: statusView({
      label: "Markdown exported",
      detail: "/tmp/circuit-review.md",
      tone: "ready",
    }),
    showDeleteOutcome: false,
    deleteCommandState: statusView({
      label: "Private artifacts deleted",
      detail: "2 private artifacts removed. 0 exported files remain outside app control.",
      tone: "ready",
    }),
    failedDeleteMeetingId: undefined,
    retryDeleteDisabled: false,
    retryDeleteTitle: "Retry deletion for the failed meeting.",
    summaryFailure: null,
    onRetryDelete: vi.fn(),
    ...overrides,
  };
}

afterEach(() => {
  cleanup();
});

describe("DesktopCommandOutcomes", () => {
  it("renders nothing when App has no command error or active outcomes", () => {
    const { container } = render(<DesktopCommandOutcomes {...desktopCommandOutcomesProps()} />);

    expect(container.firstElementChild).toBeNull();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Retry delete" })).not.toBeInTheDocument();
  });

  it("renders command errors as alerts with the existing command-error class", () => {
    render(
      <DesktopCommandOutcomes
        {...desktopCommandOutcomesProps({
          commandError: "Could not start recording: microphone access denied.",
        })}
      />,
    );

    expect(screen.getByRole("alert")).toHaveClass("command-error");
    expect(screen.getByRole("alert")).toHaveTextContent(
      "Could not start recording: microphone access denied.",
    );
  });

  it("renders export outcomes with the supplied tone class, label, and detail", () => {
    render(
      <DesktopCommandOutcomes
        {...desktopCommandOutcomesProps({
          showExportOutcome: true,
          exportCommandState: statusView({
            label: "SRT exported",
            detail: "/tmp/circuit-review.srt",
            tone: "ready",
          }),
        })}
      />,
    );

    const outcome = screen.getByRole("status");
    expect(outcome.tagName).toBe("P");
    expect(outcome).toHaveClass("command-outcome", "ready");
    expect(within(outcome).getByText("SRT exported").tagName).toBe("STRONG");
    expect(within(outcome).getByText("/tmp/circuit-review.srt").tagName).toBe("SPAN");
  });

  it("renders delete outcomes without a retry button when there is no failed meeting id", () => {
    render(
      <DesktopCommandOutcomes
        {...desktopCommandOutcomesProps({
          showDeleteOutcome: true,
          deleteCommandState: statusView({
            label: "Deleting",
            detail: "Removing local private artifacts controlled by the app.",
            tone: "active",
          }),
        })}
      />,
    );

    const outcome = screen.getByRole("status");
    expect(outcome).toHaveClass("command-outcome", "active");
    expect(within(outcome).getByText("Deleting").tagName).toBe("STRONG");
    expect(
      within(outcome).getByText("Removing local private artifacts controlled by the app.").tagName,
    ).toBe("SPAN");
    expect(within(outcome).queryByRole("button", { name: "Retry delete" })).not.toBeInTheDocument();
  });

  it("renders a retry button for failed delete outcomes", () => {
    render(
      <DesktopCommandOutcomes
        {...desktopCommandOutcomesProps({
          showDeleteOutcome: true,
          deleteCommandState: statusView({
            label: "Delete failed",
            detail: "Store cleanup failed.",
            tone: "blocked",
          }),
          failedDeleteMeetingId: "meeting-123",
        })}
      />,
    );

    const outcome = screen.getByRole("status");
    expect(outcome).toHaveClass("command-outcome", "blocked");
    expect(screen.getByRole("button", { name: "Retry delete" })).toHaveClass("button", "danger");
  });

  it("respects retry disabled/title props and delegates retry clicks", async () => {
    const user = userEvent.setup();
    const onRetryDelete = vi.fn();
    const { rerender } = render(
      <DesktopCommandOutcomes
        {...desktopCommandOutcomesProps({
          showDeleteOutcome: true,
          failedDeleteMeetingId: "meeting-123",
          retryDeleteDisabled: true,
          retryDeleteTitle: "Another command is running.",
          onRetryDelete,
        })}
      />,
    );

    expect(screen.getByRole("button", { name: "Retry delete" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Retry delete" })).toHaveAttribute(
      "title",
      "Another command is running.",
    );

    rerender(
      <DesktopCommandOutcomes
        {...desktopCommandOutcomesProps({
          showDeleteOutcome: true,
          failedDeleteMeetingId: "meeting-123",
          retryDeleteDisabled: false,
          retryDeleteTitle: "Retry deletion for the failed meeting.",
          onRetryDelete,
        })}
      />,
    );

    await user.click(screen.getByRole("button", { name: "Retry delete" }));

    expect(screen.getByRole("button", { name: "Retry delete" })).toHaveAttribute(
      "title",
      "Retry deletion for the failed meeting.",
    );
    expect(onRetryDelete).toHaveBeenCalledTimes(1);
  });

  it("renders summary failure messages with optional setup guidance", () => {
    const { rerender } = render(
      <DesktopCommandOutcomes
        {...desktopCommandOutcomesProps({
          summaryFailure: {
            message: "Summary failed because Ollama is unavailable.",
            setupGuidance: "Run ollama serve and retry summary generation.",
          },
        })}
      />,
    );

    let outcome = screen.getByRole("status");
    expect(outcome).toHaveClass("command-outcome", "blocked");
    expect(within(outcome).getByText("Summary failed because Ollama is unavailable.").tagName).toBe(
      "STRONG",
    );
    expect(within(outcome).getByText("Run ollama serve and retry summary generation.").tagName).toBe(
      "SPAN",
    );

    rerender(
      <DesktopCommandOutcomes
        {...desktopCommandOutcomesProps({
          summaryFailure: {
            message: "Summary failed because the selected meeting has no transcript segments.",
            setupGuidance: null,
          },
        })}
      />,
    );

    outcome = screen.getByRole("status");
    expect(outcome).toHaveTextContent(
      "Summary failed because the selected meeting has no transcript segments.",
    );
    expect(outcome.querySelector("span")).not.toBeInTheDocument();
  });
});
