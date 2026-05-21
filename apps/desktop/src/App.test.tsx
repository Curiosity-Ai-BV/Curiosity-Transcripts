import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";

import App from "./App";
import {
  getMockDesktopSnapshot,
  mapAnalysisDisclosure,
  mapDeleteState,
  mapExportState,
  mapModelStatus,
  mapPermissionState,
  mapRecordingState,
  searchMeetings,
} from "./commandAdapter";

afterEach(() => cleanup());

describe("desktop command-state mapping", () => {
  it("renders recording state from command trust DTOs", () => {
    expect(
      mapRecordingState({
        state: "Recording",
        permission_state: "Ready",
        recoverable: false,
        recovery_action: "",
        raw_audio_retention: "Retain",
        storage_location: { app_private_path: "meetings/circuit-review/audio" },
      }),
    ).toEqual({
      label: "Recording",
      tone: "active",
      detail: "Raw audio retained in private app storage.",
    });
  });

  it("renders permission denied as actionable unavailable capture state", () => {
    expect(mapPermissionState("MicrophoneDenied")).toEqual({
      label: "Microphone denied",
      tone: "blocked",
      detail: "Open macOS Privacy & Security and allow microphone access.",
    });
    expect(mapPermissionState("SystemAudioDenied")).toEqual({
      label: "System audio denied",
      tone: "blocked",
      detail: "Allow Screen Recording before mixed/system capture.",
    });
    expect(mapPermissionState("MicrophoneUnavailable")).toEqual({
      label: "Microphone unavailable",
      tone: "blocked",
      detail: "Connect or select a macOS input device before recording.",
    });
  });

  it("keeps recording permission guidance in the rendered detail field", () => {
    expect(
      mapRecordingState({
        state: "Recording",
        permission_state: "MicrophoneDenied",
        recoverable: false,
        recovery_action: "",
        raw_audio_retention: "Retain",
        storage_location: { app_private_path: "meetings/circuit-review/audio" },
      }),
    ).toEqual({
      label: "Microphone denied",
      tone: "blocked",
      detail: "Open macOS Privacy & Security and allow microphone access.",
    });
  });

  it("renders missing local model status without starting downloads", () => {
    expect(mapModelStatus({ kind: "missing", configuredPath: "" })).toEqual({
      label: "Whisper model missing",
      tone: "blocked",
      detail: "Choose a local model path before transcription.",
    });
  });

  it("filters search results by meeting title and transcript text", () => {
    const snapshot = getMockDesktopSnapshot();
    expect(searchMeetings(snapshot.meetings, "retention").map((meeting) => meeting.id)).toEqual([
      "circuit-review",
    ]);
    expect(searchMeetings(snapshot.meetings, "missing-term")).toEqual([]);
  });

  it("renders export and delete command outcomes honestly", () => {
    expect(mapExportState({ state: "exported", path: "/tmp/circuit-review.json" })).toEqual({
      label: "JSON exported",
      tone: "ready",
      detail: "/tmp/circuit-review.json",
    });
    expect(
      mapDeleteState({
        state: "deleted",
        deletedPrivateArtifacts: ["meetings/circuit-review/audio/mixed.wav"],
        remainingExports: ["/tmp/circuit-review.json"],
      }),
    ).toEqual({
      label: "Private artifacts deleted",
      tone: "warn",
      detail: "1 private artifact removed. 1 exported file remains outside app control.",
    });
  });

  it("discloses summary provider privacy state", () => {
    expect(
      mapAnalysisDisclosure({
        provider: "fake-local",
        modelName: "fixture-summary",
        networkUsed: false,
        disclosureRequired: false,
        disclosureConfirmed: false,
      }),
    ).toEqual({
      label: "Local summary",
      tone: "ready",
      detail: "fake-local / fixture-summary. Transcript stays on this device.",
    });
    expect(
      mapAnalysisDisclosure({
        provider: "openai-compatible",
        modelName: "DeepSeek-V3.2-Speciale",
        networkUsed: true,
        disclosureRequired: true,
        disclosureConfirmed: false,
      }),
    ).toEqual({
      label: "Hosted summary gated",
      tone: "blocked",
      detail: "Select a key and confirm transcript data disclosure before sending anything.",
    });
  });
});

describe("desktop workspace shell", () => {
  it("shows the transcript workspace as the first screen", () => {
    render(<App />);

    expect(screen.getByRole("heading", { name: "Transcript workspace" })).toBeInTheDocument();
    expect(screen.getByLabelText("Search meetings")).toBeInTheDocument();
    expect(
      within(screen.getByLabelText("Recording controls and status")).getByText(
        "Backend command wiring is not connected in this shell yet.",
      ),
    ).toBeInTheDocument();
    expect(within(screen.getByLabelText("Meetings")).getByText("Circuit Review")).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Transcript" })).toBeInTheDocument();
    expect(screen.getByText("Private storage")).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Structured summary" })).toBeInTheDocument();
    expect(screen.getByText("Settings")).toBeInTheDocument();
  });

  it("renders loading, empty, unavailable, permission-denied, transcribing, and ready states", () => {
    render(<App snapshot={getMockDesktopSnapshot("state-matrix")} />);

    expect(screen.getByText("Loading workspace")).toBeInTheDocument();
    expect(screen.queryByText("No meetings match this search.")).not.toBeInTheDocument();
    expect(screen.getByText("System audio unavailable")).toBeInTheDocument();
    expect(screen.getByText("Microphone denied")).toBeInTheDocument();
    expect(screen.getAllByText("Transcribing").length).toBeGreaterThan(0);
    expect(screen.getAllByText("Ready").length).toBeGreaterThan(0);
  });

  it("does not render developer-local absolute paths in the shell fixture", () => {
    const { container } = render(<App />);

    expect(container.textContent).not.toContain("/Users/adrian");
  });

  it("shows unavailable command controls without fabricating export or delete results", () => {
    render(<App />);

    expect(screen.getByText("Preview shell")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Export JSON" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Delete private data" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Generate summary" })).toBeDisabled();
    expect(screen.queryByText("JSON exported")).not.toBeInTheDocument();
    expect(screen.queryByText("exports/circuit-review.json")).not.toBeInTheDocument();
    expect(screen.queryByText(/private artifact removed/i)).not.toBeInTheDocument();
  });

  it("updates meeting results from search input", async () => {
    const user = userEvent.setup();
    render(<App />);

    await user.type(screen.getByLabelText("Search meetings"), "standup");

    expect(within(screen.getByLabelText("Meetings")).getByText("Design Standup")).toBeInTheDocument();
    expect(within(screen.getByLabelText("Meetings")).queryByText("Circuit Review")).not.toBeInTheDocument();
  });

  it("renders empty search results separately from loading", async () => {
    const user = userEvent.setup();
    render(<App />);

    await user.type(screen.getByLabelText("Search meetings"), "missing-term");

    expect(screen.getByText("No meetings match this search.")).toBeInTheDocument();
    expect(screen.getByText("No meeting selected.")).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Circuit Review" })).not.toBeInTheDocument();
  });

  it("labels unavailable microphone and system audio states by source", () => {
    render(
      <App
        snapshot={{
          ...getMockDesktopSnapshot(),
          capture: {
            microphone: "MicrophoneUnavailable",
            systemAudio: "SystemAudioUnavailable",
          },
        }}
      />,
    );

    expect(screen.getByText("Microphone unavailable")).toBeInTheDocument();
    expect(screen.getByText("System audio unavailable")).toBeInTheDocument();
  });

  it("marks the selected meeting for assistive technology", () => {
    render(<App />);

    expect(screen.getByRole("button", { name: /Circuit Review/ })).toHaveAttribute("aria-current", "page");
    expect(screen.getByRole("button", { name: /Circuit Review/ })).toHaveAttribute("aria-pressed", "true");
  });
});
