import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";

import type { Tone } from "./commandAdapter";
import {
  DesktopSettingsEngineStack,
  type DesktopSettingsEngineStackProps,
  type EngineStatusLine,
} from "./desktopSettingsEngineStack";

function statusLine(label: string, value: string, tone: Tone = "ready"): EngineStatusLine {
  return { label, value, tone };
}

function engineStackProps(
  overrides: Partial<DesktopSettingsEngineStackProps> = {},
): DesktopSettingsEngineStackProps {
  return {
    model: statusLine("Whisper ready", "Local model path passed validation."),
    transcription: statusLine("Transcription idle", "Select a meeting before transcription.", "muted"),
    transcriptionJob: null,
    summaryJob: null,
    microphone: statusLine("Microphone ready", "Captures private microphone audio."),
    systemAudio: statusLine("System audio ready", "Captures private system audio."),
    calendar: statusLine("Calendar ready", "Upcoming events can be attached."),
    selectedMeetingAnalysis: statusLine(
      "Summary unavailable",
      "No selected meeting.",
      "muted",
    ),
    canCancelTranscriptionJob: false,
    canRetryTranscriptionJob: false,
    canCancelSummaryJob: false,
    canRetrySummaryJob: false,
    cancelTranscriptionDisabled: false,
    retryTranscriptionDisabled: false,
    cancelSummaryDisabled: false,
    retrySummaryDisabled: false,
    cancelTranscriptionButtonTitle: "Request cancellation for the active transcription job.",
    retryTranscriptionButtonTitle: "Retry transcription for the selected meeting.",
    cancelSummaryButtonTitle: "Request cancellation for the active summary job.",
    summaryButtonTitle: "Generate a local Ollama summary for the selected meeting.",
    pendingCommand: null,
    onCancelTranscriptionJob: vi.fn(),
    onRetryTranscriptionJob: vi.fn(),
    onCancelSummaryJob: vi.fn(),
    onRetrySummaryJob: vi.fn(),
    ...overrides,
  };
}

afterEach(() => {
  cleanup();
});

describe("DesktopSettingsEngineStack", () => {
  it("renders the full base status stack with the expected class, aria label, and status text", () => {
    const { container } = render(<DesktopSettingsEngineStack {...engineStackProps()} />);
    const stack = screen.getByLabelText("Model and capture status");

    expect(container.firstElementChild).toBe(stack);
    expect(stack).toHaveClass("engine-stack");
    expect(within(stack).getByText("Whisper ready")).toBeInTheDocument();
    expect(within(stack).getByText("Local model path passed validation.")).toBeInTheDocument();
    expect(within(stack).getByText("Transcription idle")).toBeInTheDocument();
    expect(within(stack).getByText("Select a meeting before transcription.")).toBeInTheDocument();
    expect(within(stack).getByText("Microphone ready")).toBeInTheDocument();
    expect(within(stack).getByText("Captures private microphone audio.")).toBeInTheDocument();
    expect(within(stack).getByText("System audio ready")).toBeInTheDocument();
    expect(within(stack).getByText("Captures private system audio.")).toBeInTheDocument();
    expect(within(stack).getByText("Calendar ready")).toBeInTheDocument();
    expect(within(stack).getByText("Upcoming events can be attached.")).toBeInTheDocument();
    expect(within(stack).getByText("Summary unavailable")).toBeInTheDocument();
    expect(within(stack).getByText("No selected meeting.")).toBeInTheDocument();
  });

  it("omits job controls when job view data is null and omits analysis when no analysis line is supplied", () => {
    render(
      <DesktopSettingsEngineStack
        {...engineStackProps({
          selectedMeetingAnalysis: null,
          canCancelTranscriptionJob: true,
          canRetryTranscriptionJob: true,
          canCancelSummaryJob: true,
          canRetrySummaryJob: true,
        })}
      />,
    );

    expect(screen.queryByText("Transcription retryable")).not.toBeInTheDocument();
    expect(screen.queryByText("Summary retryable")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Cancel transcription" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Retry transcription" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Cancel summary" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Retry summary" })).not.toBeInTheDocument();
    expect(screen.queryByText("Summary unavailable")).not.toBeInTheDocument();
  });

  it("renders transcription and summary job status lines plus available cancel and retry controls", () => {
    render(
      <DesktopSettingsEngineStack
        {...engineStackProps({
          transcriptionJob: statusLine(
            "Transcription retryable",
            "Retry this transcription job when you are ready.",
            "warn",
          ),
          summaryJob: statusLine("Summary active", "Generating a local summary.", "active"),
          canCancelTranscriptionJob: true,
          canRetryTranscriptionJob: true,
          canCancelSummaryJob: true,
          canRetrySummaryJob: true,
        })}
      />,
    );

    expect(screen.getByText("Transcription retryable")).toBeInTheDocument();
    expect(screen.getByText("Retry this transcription job when you are ready.")).toBeInTheDocument();
    expect(screen.getByText("Summary active")).toBeInTheDocument();
    expect(screen.getByText("Generating a local summary.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Cancel transcription" })).toHaveClass("button");
    expect(screen.getByRole("button", { name: "Retry transcription" })).toHaveClass("button");
    expect(screen.getByRole("button", { name: "Cancel summary" })).toHaveClass("button");
    expect(screen.getByRole("button", { name: "Retry summary" })).toHaveClass("button");
  });

  it("calls supplied action callbacks and applies disabled and title props to job controls", async () => {
    const user = userEvent.setup();
    const onCancelTranscriptionJob = vi.fn();
    const onRetryTranscriptionJob = vi.fn();
    const onCancelSummaryJob = vi.fn();
    const onRetrySummaryJob = vi.fn();
    const { rerender } = render(
      <DesktopSettingsEngineStack
        {...engineStackProps({
          transcriptionJob: statusLine("Transcription active", "Transcribing meeting.", "active"),
          summaryJob: statusLine("Summary retryable", "Retry this summary job when you are ready.", "warn"),
          canCancelTranscriptionJob: true,
          canRetryTranscriptionJob: true,
          canCancelSummaryJob: true,
          canRetrySummaryJob: true,
          onCancelTranscriptionJob,
          onRetryTranscriptionJob,
          onCancelSummaryJob,
          onRetrySummaryJob,
        })}
      />,
    );

    expect(screen.getByRole("button", { name: "Cancel transcription" })).toHaveAttribute(
      "title",
      "Request cancellation for the active transcription job.",
    );
    expect(screen.getByRole("button", { name: "Cancel summary" })).toHaveAttribute(
      "title",
      "Request cancellation for the active summary job.",
    );
    expect(screen.getByRole("button", { name: "Retry transcription" })).toHaveAttribute(
      "title",
      "Retry transcription for the selected meeting.",
    );
    expect(screen.getByRole("button", { name: "Retry summary" })).toHaveAttribute(
      "title",
      "Generate a local Ollama summary for the selected meeting.",
    );

    await user.click(screen.getByRole("button", { name: "Cancel transcription" }));
    await user.click(screen.getByRole("button", { name: "Retry transcription" }));
    await user.click(screen.getByRole("button", { name: "Cancel summary" }));
    await user.click(screen.getByRole("button", { name: "Retry summary" }));

    expect(onCancelTranscriptionJob).toHaveBeenCalledTimes(1);
    expect(onRetryTranscriptionJob).toHaveBeenCalledTimes(1);
    expect(onCancelSummaryJob).toHaveBeenCalledTimes(1);
    expect(onRetrySummaryJob).toHaveBeenCalledTimes(1);

    rerender(
      <DesktopSettingsEngineStack
        {...engineStackProps({
          transcriptionJob: statusLine("Transcription active", "Transcribing meeting.", "active"),
          summaryJob: statusLine("Summary retryable", "Retry this summary job when you are ready.", "warn"),
          canCancelTranscriptionJob: true,
          canRetryTranscriptionJob: true,
          canCancelSummaryJob: true,
          canRetrySummaryJob: true,
          cancelTranscriptionDisabled: true,
          retryTranscriptionDisabled: true,
          cancelSummaryDisabled: true,
          retrySummaryDisabled: true,
          cancelTranscriptionButtonTitle: "Connect the desktop command surface first.",
          retryTranscriptionButtonTitle: "Retry transcription unavailable.",
          cancelSummaryButtonTitle: "Connect the desktop command surface first.",
          summaryButtonTitle: "Retry summary unavailable.",
          onCancelTranscriptionJob,
          onRetryTranscriptionJob,
          onCancelSummaryJob,
          onRetrySummaryJob,
        })}
      />,
    );

    expect(screen.getByRole("button", { name: "Cancel transcription" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Cancel transcription" })).toHaveAttribute(
      "title",
      "Connect the desktop command surface first.",
    );
    expect(screen.getByRole("button", { name: "Retry transcription" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Retry transcription" })).toHaveAttribute(
      "title",
      "Retry transcription unavailable.",
    );
    expect(screen.getByRole("button", { name: "Cancel summary" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Cancel summary" })).toHaveAttribute(
      "title",
      "Connect the desktop command surface first.",
    );
    expect(screen.getByRole("button", { name: "Retry summary" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Retry summary" })).toHaveAttribute(
      "title",
      "Retry summary unavailable.",
    );
  });

  it("swaps button labels for pending cancel and retry command states", () => {
    const { rerender } = render(
      <DesktopSettingsEngineStack
        {...engineStackProps({
          transcriptionJob: statusLine("Transcription active", "Transcribing meeting.", "active"),
          summaryJob: statusLine("Summary active", "Generating a local summary.", "active"),
          canCancelTranscriptionJob: true,
          canRetryTranscriptionJob: true,
          canCancelSummaryJob: true,
          canRetrySummaryJob: true,
          pendingCommand: "cancel-transcription",
        })}
      />,
    );

    expect(screen.getByRole("button", { name: "Canceling transcription" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Retry transcription" })).toBeInTheDocument();

    rerender(
      <DesktopSettingsEngineStack
        {...engineStackProps({
          transcriptionJob: statusLine("Transcription active", "Transcribing meeting.", "active"),
          summaryJob: statusLine("Summary active", "Generating a local summary.", "active"),
          canCancelTranscriptionJob: true,
          canRetryTranscriptionJob: true,
          canCancelSummaryJob: true,
          canRetrySummaryJob: true,
          pendingCommand: "transcribe",
        })}
      />,
    );
    expect(screen.getByRole("button", { name: "Retrying transcription" })).toBeInTheDocument();

    rerender(
      <DesktopSettingsEngineStack
        {...engineStackProps({
          transcriptionJob: statusLine("Transcription active", "Transcribing meeting.", "active"),
          summaryJob: statusLine("Summary active", "Generating a local summary.", "active"),
          canCancelTranscriptionJob: true,
          canRetryTranscriptionJob: true,
          canCancelSummaryJob: true,
          canRetrySummaryJob: true,
          pendingCommand: "cancel-summary",
        })}
      />,
    );
    expect(screen.getByRole("button", { name: "Canceling summary" })).toBeInTheDocument();

    rerender(
      <DesktopSettingsEngineStack
        {...engineStackProps({
          transcriptionJob: statusLine("Transcription active", "Transcribing meeting.", "active"),
          summaryJob: statusLine("Summary active", "Generating a local summary.", "active"),
          canCancelTranscriptionJob: true,
          canRetryTranscriptionJob: true,
          canCancelSummaryJob: true,
          canRetrySummaryJob: true,
          pendingCommand: "summary",
        })}
      />,
    );
    expect(screen.getByRole("button", { name: "Retrying summary" })).toBeInTheDocument();
  });

  it("renders exactly the selected meeting analysis status supplied by App", () => {
    render(
      <DesktopSettingsEngineStack
        {...engineStackProps({
          selectedMeetingAnalysis: statusLine(
            "Analysis fallback label",
            "Analysis fallback detail.",
            "blocked",
          ),
        })}
      />,
    );

    expect(screen.getByText("Analysis fallback label")).toBeInTheDocument();
    expect(screen.getByText("Analysis fallback detail.").closest(".status-line")).toHaveClass(
      "blocked",
    );
  });
});
