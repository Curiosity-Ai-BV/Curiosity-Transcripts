import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";

import App from "./App";
import {
  CommandFetcher,
  getMockDesktopSnapshot,
  getUnavailableDesktopSnapshot,
  loadDesktopSnapshot,
  mapAnalysisDisclosure,
  mapDeleteState,
  mapExportState,
  mapModelStatus,
  mapPermissionState,
  mapRecordingState,
  mapTranscriptionState,
  searchMeetings,
} from "./commandAdapter";

afterEach(() => cleanup());

describe("desktop command-state mapping", () => {
  it("loads the desktop snapshot through the provided command fetcher", async () => {
    const snapshot = {
      ...getMockDesktopSnapshot(),
      meetings: [],
      selectedMeetingId: null,
      commandSurface: {
        detail: "Connected to local desktop commands.",
      },
    };
    const calls: string[] = [];

    const result = await loadDesktopSnapshot({
      fetchCommand: async <T,>(command: string): Promise<T> => {
        calls.push(command);
        return snapshot as T;
      },
      previewFallback: false,
    });

    expect(calls).toEqual(["desktop_snapshot"]);
    expect(result).toEqual(snapshot);
  });

  it("only falls back to preview data when preview fallback is explicitly allowed", async () => {
    const preview = await loadDesktopSnapshot({
      fetchCommand: undefined,
      previewFallback: true,
    });

    expect(preview.commandSurface.detail).toContain("Preview");
    await expect(
      loadDesktopSnapshot({
        fetchCommand: undefined,
        previewFallback: false,
      }),
    ).rejects.toThrow("Tauri command surface is unavailable");
  });

  it("represents desktop command failures without preview meetings or fake command outcomes", () => {
    const snapshot = getUnavailableDesktopSnapshot("Desktop command loading failed: permission denied.");

    expect(snapshot.commandSurface.detail).not.toContain("Preview");
    expect(snapshot.meetings).toEqual([]);
    expect(snapshot.selectedMeetingId).toBeNull();
    expect(snapshot.recording.permission_state).toBe("MicrophoneUnavailable");
  });

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

  it("renders completed recordings as saved instead of permanently stopping", () => {
    expect(
      mapRecordingState({
        state: "Complete",
        permission_state: "Ready",
        recoverable: false,
        recovery_action: "Finalized local microphone WAV artifact.",
        raw_audio_retention: "Retain",
        storage_location: { app_private_path: "meetings/circuit-review/audio" },
      }),
    ).toEqual({
      label: "Recorded",
      tone: "ready",
      detail: "Finalized local microphone WAV artifact.",
    });
  });

  it("renders transcription command failures as visible blocked state", () => {
    expect(
      mapTranscriptionState({
        meetingId: "meeting-1",
        state: "Failed",
        failure: {
          code: "missing_model",
          message: "Whisper model is unavailable. Set CURIOSITY_WHISPER_MODEL.",
          setupGuidance: "Set CURIOSITY_WHISPER_MODEL.",
        },
      }),
    ).toEqual({
      label: "Transcription failed",
      tone: "blocked",
      detail: "Whisper model is unavailable. Set CURIOSITY_WHISPER_MODEL.",
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

  it("renders skipped private artifacts as incomplete delete cleanup", () => {
    expect(
      mapDeleteState({
        state: "deleted",
        deletedPrivateArtifacts: ["meetings/circuit-review/audio/imported.wav"],
        skippedPrivateArtifacts: ["meetings/circuit-review/audio/locked.wav"],
        remainingExports: [],
      }),
    ).toEqual({
      label: "Cleanup incomplete",
      tone: "warn",
      detail:
        "1 private artifact removed. Cleanup incomplete: 1 private artifact could not be removed. 0 exported files remain outside app control.",
    });
  });

  it("discloses summary provider privacy state", () => {
    expect(mapAnalysisDisclosure(null)).toEqual({
      label: "No summary",
      tone: "muted",
      detail: "Generate a local Ollama summary after a transcript is ready.",
    });
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
        "Preview shell: backend command wiring is not connected in this browser/dev fixture.",
      ),
    ).toBeInTheDocument();
    expect(within(screen.getByLabelText("Meetings")).getByText("Circuit Review")).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Transcript" })).toBeInTheDocument();
    expect(screen.getByText("Private storage")).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Structured summary" })).toBeInTheDocument();
    expect(screen.getByText("Settings")).toBeInTheDocument();
  });

  it("renders editable local settings from the desktop snapshot", () => {
    const snapshot = connectedSnapshot({
      settings: {
        whisperModelPath: "/models/ggml-base.en.bin",
        ollamaBaseUrl: "http://127.0.0.1:11435",
        ollamaModel: "gemma4:31b",
        exportDirectory: null,
      },
    });
    const fetchCommand: CommandFetcher = async () => snapshot as never;

    render(
      <App
        snapshot={snapshot}
        fetchCommand={fetchCommand}
      />,
    );

    expect(screen.getByLabelText("Whisper model path")).toHaveValue("/models/ggml-base.en.bin");
    expect(screen.getByLabelText("Ollama base URL")).toHaveValue("http://127.0.0.1:11435");
    expect(screen.getByLabelText("Ollama model")).toHaveValue("gemma4:31b");
    expect(screen.getByRole("button", { name: "Test path" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "Save Whisper" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "Save analysis" })).toBeEnabled();
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
    expect(screen.getByRole("button", { name: "Start recording" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Stop recording" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Transcribe" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Export JSON" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Delete private data" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Generate summary" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Start recording" })).toHaveAttribute(
      "title",
      "Preview shell: backend command wiring is not connected in this browser/dev fixture.",
    );
    expect(screen.getByRole("button", { name: "Stop recording" })).toHaveAttribute(
      "title",
      "Preview shell: backend command wiring is not connected in this browser/dev fixture.",
    );
    expect(screen.getByRole("button", { name: "Transcribe" })).toHaveAttribute(
      "title",
      "Preview shell: backend command wiring is not connected in this browser/dev fixture.",
    );
    expect(screen.getByRole("button", { name: "Export JSON" })).toHaveAttribute(
      "title",
      "Preview shell: backend command wiring is not connected in this browser/dev fixture.",
    );
    expect(screen.getByRole("button", { name: "Delete private data" })).toHaveAttribute(
      "title",
      "Preview shell: backend command wiring is not connected in this browser/dev fixture.",
    );
    expect(screen.getByRole("button", { name: "Generate summary" })).toHaveAttribute(
      "title",
      "Preview shell: backend command wiring is not connected in this browser/dev fixture.",
    );
    expect(screen.getAllByText("No summary").length).toBeGreaterThan(0);
    expect(screen.queryByText(/fake-local \/ fixture-summary/i)).not.toBeInTheDocument();
    expect(screen.queryByText("JSON exported")).not.toBeInTheDocument();
    expect(screen.queryByText("exports/circuit-review.json")).not.toBeInTheDocument();
    expect(screen.queryByText(/private artifact removed/i)).not.toBeInTheDocument();
  });

  it("does not imply connected recording support when desktop commands are read-only", () => {
    render(
      <App
        snapshot={{
          ...getMockDesktopSnapshot(),
          commandSurface: {
            detail: "Connected to local desktop commands.",
          },
          recording: {
            meeting_id: "",
            recording_id: null,
            state: "Interrupted",
            permission_state: "MicrophoneUnavailable",
            storage_location: { app_private_path: "/tmp/curiosity" },
            raw_audio_retention: "Retain",
            recoverable: false,
            recovery_action: "Recording commands are not wired into the desktop shell yet.",
          },
          capture: {
            microphone: "MicrophoneUnavailable",
            systemAudio: "SystemAudioUnavailable",
          },
        }}
      />,
    );

    expect(screen.getAllByText("Microphone unavailable").length).toBeGreaterThan(0);
    expect(screen.queryByText("Paused")).not.toBeInTheDocument();
    expect(screen.queryByText("Raw audio retained in private app storage.")).not.toBeInTheDocument();
    expect(screen.getByText("Recording commands are not wired into the desktop shell yet.")).toBeInTheDocument();
  });

  it("shows transcription failure returned by desktop commands", () => {
    render(
      <App
        snapshot={{
          ...getMockDesktopSnapshot(),
          commandSurface: {
            detail: "Connected to local desktop commands.",
          },
          transcription: {
            meetingId: "circuit-review",
            state: "Failed",
            failure: {
              code: "missing_model",
              message: "Whisper model is unavailable. Set CURIOSITY_WHISPER_MODEL.",
              setupGuidance: "Set CURIOSITY_WHISPER_MODEL.",
            },
          },
        }}
      />,
    );

    expect(screen.getAllByText("Transcription failed").length).toBeGreaterThan(0);
    expect(screen.getByText("Whisper model is unavailable. Set CURIOSITY_WHISPER_MODEL.")).toBeInTheDocument();
  });

  it("updates meeting results from search input", async () => {
    const user = userEvent.setup();
    render(<App />);

    await user.type(screen.getByLabelText("Search meetings"), "standup");

    expect(within(screen.getByLabelText("Meetings")).getByText("Design Standup")).toBeInTheDocument();
    expect(within(screen.getByLabelText("Meetings")).queryByText("Circuit Review")).not.toBeInTheDocument();
  });

  it("filters connected search through desktop command results", async () => {
    const user = userEvent.setup();
    const initial = connectedSnapshot();
    const calls: Array<{ command: string; args?: Record<string, unknown> }> = [];
    const fetchCommand: CommandFetcher = async (command, args) => {
      calls.push({ command, args });
      if (command === "search_meetings") {
        return [{ meeting_id: "design-standup", title: "Design Standup" }] as never;
      }
      return initial as never;
    };

    render(<App snapshot={initial} fetchCommand={fetchCommand} />);

    await user.type(screen.getByLabelText("Search meetings"), "layout");

    await waitFor(() =>
      expect(calls).toContainEqual({
        command: "search_meetings",
        args: { query: "layout" },
      }),
    );
    expect(within(screen.getByLabelText("Meetings")).getByText("Design Standup")).toBeInTheDocument();
    expect(within(screen.getByLabelText("Meetings")).queryByText("Circuit Review")).not.toBeInTheDocument();

    await user.clear(screen.getByLabelText("Search meetings"));

    expect(within(screen.getByLabelText("Meetings")).getByText("Circuit Review")).toBeInTheDocument();
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

  it("uses the backend-selected meeting when an async desktop snapshot replaces loading state", () => {
    const loading = getUnavailableDesktopSnapshot("Loading local desktop commands.");
    const loaded = {
      ...getMockDesktopSnapshot(),
      commandSurface: { detail: "Connected to local desktop commands." },
      selectedMeetingId: "design-standup",
    };
    const { rerender } = render(<App snapshot={{ ...loading, loading: true }} />);

    rerender(<App snapshot={loaded} />);

    expect(screen.getByRole("button", { name: /Design Standup/ })).toHaveAttribute("aria-current", "page");
    expect(screen.getByRole("heading", { name: "Design Standup" })).toBeInTheDocument();
  });

  it("starts microphone recording with the optional title and replaces the snapshot", async () => {
    const user = userEvent.setup();
    const calls: Array<{ command: string; args?: Record<string, unknown> }> = [];
    const initial = connectedSnapshot({
      recording: {
        ...getMockDesktopSnapshot().recording,
        state: "Complete",
        recovery_action: "Previous local microphone WAV artifact is saved.",
      },
    });
    const returned = connectedSnapshot({
      selectedMeetingId: "design-standup",
      recording: {
        ...initial.recording,
        meeting_id: "design-standup",
        recording_id: "recording-design-standup",
        state: "Recording",
        recovery_action: "",
        storage_location: { app_private_path: "meetings/design-standup/audio" },
      },
    });
    const fetchCommand: CommandFetcher = async (command, args) => {
      calls.push({ command, args });
      return returned as never;
    };

    render(<App snapshot={initial} fetchCommand={fetchCommand} />);

    await user.type(screen.getByLabelText("Recording title"), "MVP sync");
    await user.click(screen.getByRole("button", { name: "Start recording" }));

    expect(calls).toEqual([{ command: "start_microphone_recording", args: { title: "MVP sync" } }]);
    expect(screen.getByRole("heading", { name: "Design Standup" })).toBeInTheDocument();
    expect(screen.getAllByText("Recording").length).toBeGreaterThan(0);
    expect(screen.getAllByText("meetings/design-standup/audio").length).toBeGreaterThan(0);
  });

  it("enables stop for active recordings and disables start until the returned snapshot lands", async () => {
    const user = userEvent.setup();
    const calls: Array<{ command: string; args?: Record<string, unknown> }> = [];
    const initial = connectedSnapshot();
    const returned = connectedSnapshot({
      recording: {
        ...initial.recording,
        state: "Complete",
        recovery_action: "Finalized local microphone WAV artifact.",
      },
    });
    const fetchCommand: CommandFetcher = async (command, args) => {
      calls.push({ command, args });
      return returned as never;
    };

    render(<App snapshot={initial} fetchCommand={fetchCommand} />);

    expect(screen.getByRole("button", { name: "Start recording" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Stop recording" })).toBeEnabled();

    await user.click(screen.getByRole("button", { name: "Stop recording" }));

    expect(calls).toEqual([{ command: "stop_microphone_recording", args: undefined }]);
    expect(screen.getByText("Finalized local microphone WAV artifact.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Start recording" })).toBeEnabled();
  });

  it("transcribes the selected meeting through the desktop command and shows returned failures", async () => {
    const user = userEvent.setup();
    const calls: Array<{ command: string; args?: Record<string, unknown> }> = [];
    const initial = connectedSnapshot({
      recording: {
        ...getMockDesktopSnapshot().recording,
        state: "Complete",
        recovery_action: "Previous local microphone WAV artifact is saved.",
      },
    });
    const returned = connectedSnapshot({
      transcription: {
        meetingId: "circuit-review",
        state: "Failed",
        failure: {
          code: "missing_model",
          message: "Whisper model is unavailable. Set CURIOSITY_WHISPER_MODEL.",
          setupGuidance: "Set CURIOSITY_WHISPER_MODEL.",
        },
      },
    });
    const fetchCommand: CommandFetcher = async (command, args) => {
      calls.push({ command, args });
      return returned as never;
    };

    render(<App snapshot={initial} fetchCommand={fetchCommand} />);

    await user.click(screen.getByRole("button", { name: "Transcribe" }));

    expect(calls).toEqual([{ command: "transcribe_meeting", args: { meetingId: "circuit-review" } }]);
    expect(screen.getAllByText("Transcription failed").length).toBeGreaterThan(0);
    expect(screen.getByText("Whisper model is unavailable. Set CURIOSITY_WHISPER_MODEL.")).toBeInTheDocument();
  });

  it("renames the selected meeting through the desktop command", async () => {
    const user = userEvent.setup();
    const calls: Array<{ command: string; args?: Record<string, unknown> }> = [];
    const initial = connectedSnapshot();
    const returned = connectedSnapshot({
      meetings: initial.meetings.map((meeting) =>
        meeting.id === "circuit-review" ? { ...meeting, title: "Renamed Planning" } : meeting,
      ),
      selectedMeetingId: "circuit-review",
    });
    const fetchCommand: CommandFetcher = async (command, args) => {
      calls.push({ command, args });
      return returned as never;
    };

    render(<App snapshot={initial} fetchCommand={fetchCommand} />);

    await user.clear(screen.getByLabelText("Selected meeting title"));
    await user.type(screen.getByLabelText("Selected meeting title"), "Renamed Planning");
    await user.click(screen.getByRole("button", { name: "Rename" }));

    expect(calls).toEqual([
      {
        command: "rename_meeting",
        args: {
          meetingId: "circuit-review",
          title: "Renamed Planning",
        },
      },
    ]);
    expect(screen.getByRole("heading", { name: "Renamed Planning" })).toBeInTheDocument();
  });

  it("exports the selected meeting through the desktop command", async () => {
    const user = userEvent.setup();
    const calls: Array<{ command: string; args?: Record<string, unknown> }> = [];
    const initial = connectedSnapshot();
    const returned = connectedSnapshot({
      meetings: initial.meetings.map((meeting) =>
        meeting.id === "circuit-review"
          ? {
              ...meeting,
              exportState: {
                state: "exported",
                meetingId: "circuit-review",
                path: "/tmp/circuit-review.json",
              },
            }
          : meeting,
      ),
      exportCommand: {
        state: "exported",
        meetingId: "circuit-review",
        path: "/tmp/circuit-review.json",
      },
    });
    const fetchCommand: CommandFetcher = async (command, args) => {
      calls.push({ command, args });
      return returned as never;
    };

    render(<App snapshot={initial} fetchCommand={fetchCommand} />);

    await user.click(screen.getByRole("button", { name: "Export JSON" }));

    expect(calls).toEqual([
      {
        command: "export_meeting_json",
        args: { meetingId: "circuit-review" },
      },
    ]);
    expect(screen.getAllByText("JSON exported").length).toBeGreaterThan(0);
    expect(screen.getAllByText("/tmp/circuit-review.json").length).toBeGreaterThan(0);
  });

  it("deletes private data through the desktop command while showing remaining exports", async () => {
    const user = userEvent.setup();
    const calls: Array<{ command: string; args?: Record<string, unknown> }> = [];
    const initial = connectedSnapshot();
    const returned = connectedSnapshot({
      meetings: initial.meetings.filter((meeting) => meeting.id !== "circuit-review"),
      selectedMeetingId: "design-standup",
      deleteCommand: {
        state: "deleted",
        meetingId: "circuit-review",
        deletedPrivateArtifacts: ["meetings/circuit-review/audio/imported.wav"],
        remainingExports: ["/tmp/circuit-review.json"],
      },
    });
    const fetchCommand: CommandFetcher = async (command, args) => {
      calls.push({ command, args });
      return returned as never;
    };

    render(<App snapshot={initial} fetchCommand={fetchCommand} />);

    await user.click(screen.getByRole("button", { name: "Delete private data" }));

    expect(calls).toEqual([
      {
        command: "delete_meeting",
        args: { meetingId: "circuit-review" },
      },
    ]);
    expect(within(screen.getByLabelText("Meetings")).queryByText("Circuit Review")).not.toBeInTheDocument();
    expect(screen.getByText("Private artifacts deleted")).toBeInTheDocument();
    expect(screen.getByText("1 private artifact removed. 1 exported file remains outside app control.")).toBeInTheDocument();
  });

  it("retries a failed delete through the preserved command meeting id", async () => {
    const user = userEvent.setup();
    const calls: Array<{ command: string; args?: Record<string, unknown> }> = [];
    const initial = connectedSnapshot({
      selectedMeetingId: "design-standup",
      deleteCommand: {
        state: "failed",
        meetingId: "circuit-review",
        message: "private artifact is locked",
      },
    });
    const returned = connectedSnapshot({
      selectedMeetingId: "design-standup",
      deleteCommand: {
        state: "deleted",
        meetingId: "circuit-review",
        deletedPrivateArtifacts: ["meetings/circuit-review/audio/imported.wav"],
        remainingExports: [],
      },
    });
    const fetchCommand: CommandFetcher = async (command, args) => {
      calls.push({ command, args });
      return returned as never;
    };

    render(<App snapshot={initial} fetchCommand={fetchCommand} />);

    expect(screen.getByRole("heading", { name: "Design Standup" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Retry delete" }));

    expect(calls).toEqual([
      {
        command: "delete_meeting",
        args: { meetingId: "circuit-review" },
      },
    ]);
    expect(screen.getByText("Private artifacts deleted")).toBeInTheDocument();
  });

  it("tests and saves the configured Whisper path through desktop commands", async () => {
    const user = userEvent.setup();
    const calls: Array<{ command: string; args?: Record<string, unknown> }> = [];
    const initial = connectedSnapshot({
      settings: {
        whisperModelPath: "",
        ollamaBaseUrl: "http://127.0.0.1:11434",
        ollamaModel: "qwen3.6:27b",
        exportDirectory: null,
      },
    });
    const saved = connectedSnapshot({
      model: {
        kind: "ready",
        configuredPath: "/models/ggml-base.en.bin",
      },
      settings: {
        whisperModelPath: "/models/ggml-base.en.bin",
        ollamaBaseUrl: "http://127.0.0.1:11434",
        ollamaModel: "qwen3.6:27b",
        exportDirectory: null,
      },
    });
    const fetchCommand: CommandFetcher = async (command, args) => {
      calls.push({ command, args });
      if (command === "test_whisper_model_path") {
        return {
          state: "Valid",
          message: "Whisper model path is readable.",
          setupGuidance: "",
        } as never;
      }
      return saved as never;
    };

    render(<App snapshot={initial} fetchCommand={fetchCommand} />);

    await user.type(screen.getByLabelText("Whisper model path"), "/models/ggml-base.en.bin");
    await user.click(screen.getByRole("button", { name: "Test path" }));
    expect(await screen.findByText("Whisper model path is readable.")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Save Whisper" }));

    expect(calls).toEqual([
      {
        command: "test_whisper_model_path",
        args: { path: "/models/ggml-base.en.bin" },
      },
      {
        command: "save_whisper_model_path",
        args: { whisperModelPath: "/models/ggml-base.en.bin" },
      },
    ]);
    expect(screen.getByText("Whisper model path saved.")).toBeInTheDocument();
    expect(screen.getByLabelText("Whisper model path")).toHaveValue("/models/ggml-base.en.bin");
  });

  it("saves local analysis settings through the desktop command", async () => {
    const user = userEvent.setup();
    const calls: Array<{ command: string; args?: Record<string, unknown> }> = [];
    const initial = connectedSnapshot({
      settings: {
        whisperModelPath: "",
        ollamaBaseUrl: "http://127.0.0.1:11434",
        ollamaModel: "qwen3.6:27b",
        exportDirectory: null,
      },
    });
    const returned = connectedSnapshot({
      settings: {
        whisperModelPath: "",
        ollamaBaseUrl: "http://127.0.0.1:11435",
        ollamaModel: "gemma4:31b",
        exportDirectory: null,
      },
    });
    const fetchCommand: CommandFetcher = async (command, args) => {
      calls.push({ command, args });
      return returned as never;
    };

    render(<App snapshot={initial} fetchCommand={fetchCommand} />);

    await user.clear(screen.getByLabelText("Ollama base URL"));
    await user.type(screen.getByLabelText("Ollama base URL"), "http://127.0.0.1:11435");
    await user.clear(screen.getByLabelText("Ollama model"));
    await user.type(screen.getByLabelText("Ollama model"), "gemma4:31b");
    await user.click(screen.getByRole("button", { name: "Save analysis" }));

    expect(calls).toEqual([
      {
        command: "save_analysis_settings",
        args: {
          ollamaBaseUrl: "http://127.0.0.1:11435",
          ollamaModel: "gemma4:31b",
        },
      },
    ]);
    expect(screen.getByLabelText("Ollama model")).toHaveValue("gemma4:31b");
  });

  it("tests configured Ollama reachability from settings", async () => {
    const user = userEvent.setup();
    const calls: Array<{ command: string; args?: Record<string, unknown> }> = [];
    const initial = connectedSnapshot({
      settings: {
        whisperModelPath: "",
        ollamaBaseUrl: "http://127.0.0.1:11434",
        ollamaModel: "qwen3.6:27b",
        exportDirectory: null,
      },
    });
    const fetchCommand: CommandFetcher = async (command, args) => {
      calls.push({ command, args });
      return {
        state: "Available",
        message: "Ollama is reachable and qwen3.6:27b is installed.",
        setupGuidance: "",
      } as never;
    };

    render(<App snapshot={initial} fetchCommand={fetchCommand} />);

    await user.click(screen.getByRole("button", { name: "Test Ollama" }));

    expect(calls).toEqual([
      {
        command: "test_ollama_connection",
        args: {
          baseUrl: "http://127.0.0.1:11434",
          model: "qwen3.6:27b",
        },
      },
    ]);
    expect(screen.getByText("Ollama is reachable and qwen3.6:27b is installed.")).toBeInTheDocument();
  });

  it("clears successful Ollama reachability feedback when tested inputs change", async () => {
    const user = userEvent.setup();
    const initial = connectedSnapshot({
      settings: {
        whisperModelPath: "",
        ollamaBaseUrl: "http://127.0.0.1:11434",
        ollamaModel: "qwen3.6:27b",
        exportDirectory: null,
      },
    });
    const fetchCommand: CommandFetcher = async () =>
      ({
        state: "Available",
        message: "Ollama is reachable and qwen3.6:27b is installed.",
        setupGuidance: "",
      }) as never;

    render(<App snapshot={initial} fetchCommand={fetchCommand} />);

    await user.click(screen.getByRole("button", { name: "Test Ollama" }));
    await user.clear(screen.getByLabelText("Ollama model"));
    await user.type(screen.getByLabelText("Ollama model"), "gemma4:31b");

    expect(screen.queryByText("Ollama is reachable and qwen3.6:27b is installed.")).not.toBeInTheDocument();
  });

  it("generates a local summary for a selected meeting with transcript segments", async () => {
    const user = userEvent.setup();
    const calls: Array<{ command: string; args?: Record<string, unknown> }> = [];
    const initial = connectedSnapshot();
    const returned = connectedSnapshot({
      meetings: initial.meetings.map((meeting) =>
        meeting.id === "circuit-review"
          ? {
              ...meeting,
              analysis: {
                provider: "ollama",
                modelName: "qwen3.6:27b",
                networkUsed: false,
                disclosureRequired: false,
                disclosureConfirmed: false,
                summary: "Local Ollama summary",
                createdAtMs: 1_700_000_002_000,
                promptTemplateVersion: "summary-v1",
              },
            }
          : meeting,
      ),
      analysisCommand: {
        meetingId: "circuit-review",
        state: "Complete",
        analysis: {
          provider: "ollama",
          modelName: "qwen3.6:27b",
          networkUsed: false,
          summary: "Local Ollama summary",
        },
        failure: null,
      },
    });
    const fetchCommand: CommandFetcher = async (command, args) => {
      calls.push({ command, args });
      return returned as never;
    };

    render(<App snapshot={initial} fetchCommand={fetchCommand} />);

    await user.click(screen.getByRole("button", { name: "Generate summary" }));

    expect(calls).toEqual([
      {
        command: "generate_summary",
        args: { meetingId: "circuit-review" },
      },
    ]);
    expect(screen.getByText("Local Ollama summary")).toBeInTheDocument();
    expect(screen.getAllByText(/ollama \/ qwen3.6:27b/i).length).toBeGreaterThan(0);
  });

  it("shows Ollama setup failures returned by summary generation", async () => {
    const user = userEvent.setup();
    const initial = connectedSnapshot();
    const returned = connectedSnapshot({
      analysisCommand: {
        meetingId: "circuit-review",
        state: "Failed",
        analysis: null,
        failure: {
          code: "ollama_unavailable",
          message: "Ollama is unavailable: connection refused",
          setupGuidance: "Start Ollama with `ollama serve`, install the selected model, then retry.",
        },
      },
    });
    const fetchCommand: CommandFetcher = async () => returned as never;

    render(<App snapshot={initial} fetchCommand={fetchCommand} />);

    await user.click(screen.getByRole("button", { name: "Generate summary" }));

    expect(screen.getByText("Ollama is unavailable: connection refused")).toBeInTheDocument();
    expect(screen.getByText("Start Ollama with `ollama serve`, install the selected model, then retry.")).toBeInTheDocument();
  });

  it("keeps summary generation disabled when the selected meeting has no transcript segments", () => {
    const initial = connectedSnapshot({
      meetings: [
        {
          ...getMockDesktopSnapshot().meetings[0],
          segments: [],
          transcriptText: "",
          transcriptState: "Unavailable",
        },
      ],
      selectedMeetingId: "circuit-review",
    });

    render(<App snapshot={initial} fetchCommand={async () => initial as never} />);

    expect(screen.getByRole("button", { name: "Generate summary" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Generate summary" })).toHaveAttribute(
      "title",
      "Generate a transcript before requesting a summary.",
    );
  });

  it("shows in-flight command state and prevents double submitting recording start", async () => {
    const user = userEvent.setup();
    const calls: string[] = [];
    let finishCommand!: () => void;
    const initial = connectedSnapshot({
      recording: {
        ...getMockDesktopSnapshot().recording,
        state: "Complete",
        recovery_action: "Previous local microphone WAV artifact is saved.",
      },
    });
    const returned = connectedSnapshot();
    const pendingSnapshot = new Promise<typeof returned>((resolve) => {
      finishCommand = () => resolve(returned);
    });
    const fetchCommand: CommandFetcher = async <T,>(command: string): Promise<T> => {
      calls.push(command);
      return pendingSnapshot as Promise<T>;
    };

    render(<App snapshot={initial} fetchCommand={fetchCommand} />);

    await user.click(screen.getByRole("button", { name: "Start recording" }));

    expect(screen.getByRole("button", { name: "Starting recording" })).toBeDisabled();
    await user.click(screen.getByRole("button", { name: "Starting recording" }));
    expect(calls).toEqual(["start_microphone_recording"]);

    finishCommand();
    expect(await screen.findByRole("button", { name: "Stop recording" })).toBeEnabled();
  });

  it("shows invocation errors without replacing the current snapshot", async () => {
    const user = userEvent.setup();
    const initial = connectedSnapshot({
      recording: {
        ...getMockDesktopSnapshot().recording,
        state: "Complete",
        recovery_action: "Previous local microphone WAV artifact is saved.",
        storage_location: { app_private_path: "meetings/circuit-review/audio" },
      },
    });
    const fetchCommand: CommandFetcher = async () => {
      throw new Error("microphone permission denied");
    };

    render(<App snapshot={initial} fetchCommand={fetchCommand} />);

    await user.click(screen.getByRole("button", { name: "Start recording" }));

    expect(screen.getByRole("alert")).toHaveTextContent("microphone permission denied");
    expect(screen.getByRole("heading", { name: "Circuit Review" })).toBeInTheDocument();
    expect(screen.getAllByText("meetings/circuit-review/audio").length).toBeGreaterThan(0);
  });
});

function connectedSnapshot(overrides: Partial<ReturnType<typeof getMockDesktopSnapshot>> = {}) {
  const base = getMockDesktopSnapshot();
  return {
    ...base,
    commandSurface: {
      detail: "Connected to local desktop commands.",
    },
    capture: {
      microphone: "Ready" as const,
      systemAudio: "SystemAudioUnavailable" as const,
    },
    model: {
      kind: "ready" as const,
      configuredPath: "~/Library/Application Support/Curiosity/models/base.en.bin",
    },
    ...overrides,
  };
}
