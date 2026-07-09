import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";

import { RecordingControls, type RecordingControlsProps } from "./desktopRecordingControls";

function recordingControlsProps(overrides: Partial<RecordingControlsProps> = {}): RecordingControlsProps {
  return {
    recording: {
      label: "Ready to record",
      tone: "ready",
      detail: "Microphone and system audio are available.",
    },
    recordingTitle: "Circuit review",
    importWavPath: "/tmp/circuit-review.wav",
    recordingTitleDisabled: false,
    importWavPathDisabled: false,
    chooseWavDisabled: false,
    startDisabled: false,
    importDisabled: false,
    stopDisabled: false,
    chooseWavButtonTitle: "Choose a local WAV source file.",
    startButtonTitle: "Start desktop recording.",
    importButtonTitle: "Import the WAV file into private app storage.",
    stopButtonTitle: "No active desktop recording to stop.",
    storagePath: "meetings/circuit-review/audio",
    pendingCommand: null,
    onRecordingTitleChange: vi.fn(),
    onImportWavPathChange: vi.fn(),
    onChooseWav: vi.fn(),
    onStartRecording: vi.fn(),
    onImportWav: vi.fn(),
    onStopRecording: vi.fn(),
    ...overrides,
  };
}

afterEach(() => {
  cleanup();
});

describe("RecordingControls", () => {
  it("renders status label/detail and storage path with expected classes/labels", () => {
    const { container } = render(
      <RecordingControls
        {...recordingControlsProps({
          recording: {
            label: "Live capture",
            tone: "active",
            detail: "Raw audio retained in private app storage.",
          },
        })}
      />,
    );

    expect(screen.getByLabelText("Recording controls and status")).toHaveClass("recording-strip");
    expect(screen.getByRole("heading", { name: "Recording" })).toBeInTheDocument();
    expect(screen.getByText("Live capture")).toHaveClass("status-pill", "active");
    expect(screen.getByText("Raw audio retained in private app storage.")).toBeInTheDocument();
    expect(screen.getByText("meetings/circuit-review/audio")).toHaveClass("recording-path");
    expect(container.querySelector(".strip-primary .icon-frame")).toHaveClass("icon-frame", "active");
  });

  it("renders both controlled inputs and propagates recording title and WAV path changes", () => {
    const onRecordingTitleChange = vi.fn();
    const onImportWavPathChange = vi.fn();

    render(
      <RecordingControls
        {...recordingControlsProps({
          recordingTitle: "Existing title",
          importWavPath: "/existing/audio.wav",
          onRecordingTitleChange,
          onImportWavPathChange,
        })}
      />,
    );

    const titleInput = screen.getByLabelText("Recording title");
    const wavPathInput = screen.getByLabelText("WAV source path");

    expect(titleInput).toHaveValue("Existing title");
    expect(titleInput).toHaveAttribute("placeholder", "Optional meeting title");
    expect(wavPathInput).toHaveValue("/existing/audio.wav");
    expect(wavPathInput).toHaveAttribute("placeholder", "/path/to/audio.wav");

    fireEvent.change(titleInput, { target: { value: "Updated title" } });
    fireEvent.change(wavPathInput, { target: { value: "/updated/audio.wav" } });

    expect(onRecordingTitleChange).toHaveBeenCalledWith("Updated title");
    expect(onImportWavPathChange).toHaveBeenCalledWith("/updated/audio.wav");
  });

  it("calls choose/start/import/stop callbacks from the right buttons", async () => {
    const user = userEvent.setup();
    const onChooseWav = vi.fn();
    const onStartRecording = vi.fn();
    const onImportWav = vi.fn();
    const onStopRecording = vi.fn();

    render(
      <RecordingControls
        {...recordingControlsProps({
          onChooseWav,
          onStartRecording,
          onImportWav,
          onStopRecording,
        })}
      />,
    );

    await user.click(screen.getByRole("button", { name: "Choose WAV" }));
    await user.click(screen.getByRole("button", { name: "Start recording" }));
    await user.click(screen.getByRole("button", { name: "Import WAV" }));
    await user.click(screen.getByRole("button", { name: "Stop recording" }));

    expect(onChooseWav).toHaveBeenCalledTimes(1);
    expect(onStartRecording).toHaveBeenCalledTimes(1);
    expect(onImportWav).toHaveBeenCalledTimes(1);
    expect(onStopRecording).toHaveBeenCalledTimes(1);
  });

  it("applies disabled/title props to fields and buttons", () => {
    render(
      <RecordingControls
        {...recordingControlsProps({
          recordingTitleDisabled: true,
          importWavPathDisabled: true,
          chooseWavDisabled: true,
          startDisabled: true,
          importDisabled: true,
          stopDisabled: true,
          chooseWavButtonTitle: "Choose disabled",
          startButtonTitle: "Start disabled",
          importButtonTitle: "Import disabled",
          stopButtonTitle: "Stop disabled",
        })}
      />,
    );

    expect(screen.getByLabelText("Recording title")).toBeDisabled();
    expect(screen.getByLabelText("WAV source path")).toBeDisabled();
    expect(screen.getByRole("button", { name: "Choose WAV" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Choose WAV" })).toHaveAttribute("title", "Choose disabled");
    expect(screen.getByRole("button", { name: "Start recording" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Start recording" })).toHaveAttribute("title", "Start disabled");
    expect(screen.getByRole("button", { name: "Import WAV" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Import WAV" })).toHaveAttribute("title", "Import disabled");
    expect(screen.getByRole("button", { name: "Stop recording" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Stop recording" })).toHaveAttribute("title", "Stop disabled");
  });

  it("renders pending labels for choose/start/import/stop states", () => {
    const { rerender } = render(<RecordingControls {...recordingControlsProps({ pendingCommand: "choose-wav" })} />);

    expect(screen.getByRole("button", { name: "Choosing WAV" })).toBeInTheDocument();

    rerender(<RecordingControls {...recordingControlsProps({ pendingCommand: "start" })} />);
    expect(screen.getByRole("button", { name: "Starting recording" })).toBeInTheDocument();

    rerender(<RecordingControls {...recordingControlsProps({ pendingCommand: "import" })} />);
    expect(screen.getByRole("button", { name: "Importing WAV" })).toBeInTheDocument();

    rerender(<RecordingControls {...recordingControlsProps({ pendingCommand: "stop" })} />);
    expect(screen.getByRole("button", { name: "Stopping recording" })).toBeInTheDocument();
  });
});
