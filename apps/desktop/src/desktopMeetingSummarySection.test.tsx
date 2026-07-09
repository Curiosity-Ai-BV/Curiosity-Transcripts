import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";

import type { StatusView } from "./commandAdapter";
import { MeetingSummarySection, type MeetingSummarySectionProps } from "./desktopMeetingSummarySection";

function statusView(overrides: Partial<StatusView> = {}): StatusView {
  return {
    label: "AI summary",
    detail: "Generated locally with Ollama.",
    tone: "ready",
    ...overrides,
  };
}

function meetingSummarySectionProps(
  overrides: Partial<MeetingSummarySectionProps> = {},
): MeetingSummarySectionProps {
  return {
    disclosure: statusView(),
    summaryText: "Key decisions: use local processing and keep transcript exports private.",
    summaryDisabled: false,
    summaryButtonTitle: "Generate a structured summary for this meeting.",
    pendingCommand: null,
    onGenerateSummary: vi.fn(),
    ...overrides,
  };
}

function statusLineFor(label: string): Element | null {
  return screen.getByText(label).closest(".status-line");
}

afterEach(() => {
  cleanup();
});

describe("MeetingSummarySection", () => {
  it("renders the structured-summary heading, disclosure status line, summary text, and generate button", () => {
    render(<MeetingSummarySection {...meetingSummarySectionProps()} />);

    expect(screen.getByLabelText("Structured summary")).toHaveClass("summary-section");
    expect(screen.getByRole("heading", { name: "Structured summary" })).toBeInTheDocument();
    expect(screen.getByText("AI summary")).toBeInTheDocument();
    expect(screen.getByText("Generated locally with Ollama.")).toBeInTheDocument();
    expect(statusLineFor("AI summary")).toHaveClass("status-line", "ready");
    expect(screen.getByText("Key decisions: use local processing and keep transcript exports private.")).toHaveClass(
      "summary-text",
    );
    expect(screen.getByRole("button", { name: "Generate summary" })).toHaveClass("button");
  });

  it("omits the summary text when summaryText is null or an empty string", () => {
    const { rerender } = render(<MeetingSummarySection {...meetingSummarySectionProps({ summaryText: null })} />);

    expect(
      screen.queryByText("Key decisions: use local processing and keep transcript exports private."),
    ).not.toBeInTheDocument();
    expect(document.querySelector(".summary-text")).not.toBeInTheDocument();

    rerender(<MeetingSummarySection {...meetingSummarySectionProps({ summaryText: "" })} />);
    expect(document.querySelector(".summary-text")).not.toBeInTheDocument();
  });

  it("calls onGenerateSummary from the generate button", async () => {
    const user = userEvent.setup();
    const onGenerateSummary = vi.fn();

    render(<MeetingSummarySection {...meetingSummarySectionProps({ onGenerateSummary })} />);

    await user.click(screen.getByRole("button", { name: "Generate summary" }));

    expect(onGenerateSummary).toHaveBeenCalledTimes(1);
  });

  it("applies disabled and title props to the generate button", () => {
    render(
      <MeetingSummarySection
        {...meetingSummarySectionProps({
          summaryDisabled: true,
          summaryButtonTitle: "Summary generation is unavailable.",
        })}
      />,
    );

    expect(screen.getByRole("button", { name: "Generate summary" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Generate summary" })).toHaveAttribute(
      "title",
      "Summary generation is unavailable.",
    );
  });

  it("renders the pending label while a summary command is active", () => {
    render(<MeetingSummarySection {...meetingSummarySectionProps({ pendingCommand: "summary" })} />);

    expect(screen.getByRole("button", { name: "Generating summary" })).toBeInTheDocument();
  });
});
