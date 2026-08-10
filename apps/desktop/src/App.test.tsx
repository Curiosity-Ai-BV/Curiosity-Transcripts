import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";

import packageInfo from "../package.json";
import App from "./App";
import {
  CommandFetcher,
  DesktopCommandFacade,
  getMockDesktopSnapshot,
  getUnavailableDesktopSnapshot,
  loadDesktopSnapshot,
  mapAnalysisDisclosure,
  mapCommandJobState,
  mapDeleteState,
  mapExportState,
  mapModelStatus,
  mapPermissionState,
  mapRecordingState,
  mapTranscriptionState,
  searchMeetings,
} from "./commandAdapter";

const dialogOpen = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: dialogOpen,
}));

afterEach(() => {
  cleanup();
  dialogOpen.mockReset();
  vi.restoreAllMocks();
});

describe("desktop command-state mapping", () => {
  it("loads the desktop snapshot through the provided command fetcher", async () => {
    const snapshot = {
      ...getMockDesktopSnapshot(),
      meetings: [],
      selectedMeetingId: null,
      commandSurface: {
        ready: true,
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
        recovery_action: "Finalized local microphone and system audio WAV artifacts.",
        raw_audio_retention: "Retain",
        storage_location: { app_private_path: "meetings/circuit-review/audio" },
      }),
    ).toEqual({
      label: "Recorded",
      tone: "ready",
      detail: "Finalized local microphone and system audio WAV artifacts.",
    });
  });

  it("renders idle recording state without microphone approval guidance", () => {
    expect(
      mapRecordingState({
        state: "Idle",
        permission_state: "Ready",
        recoverable: false,
        recovery_action: "Start a desktop recording to create private microphone and system audio WAV artifacts.",
        raw_audio_retention: "Retain",
        storage_location: { app_private_path: "/tmp/curiosity" },
      }),
    ).toEqual({
      label: "Recording idle",
      tone: "muted",
      detail: "Start a desktop recording to create private microphone and system audio WAV artifacts.",
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

  it("renders recovered durable command jobs with recovery detail", () => {
    expect(
      mapCommandJobState({
        id: "transcription-circuit-review-1700000001000",
        kind: "Transcription",
        meetingId: "circuit-review",
        state: "Recovery",
        cancelRequested: false,
        startedAtMs: 1_700_000_001_000,
        lastError: "transcription worker was not running after app restart",
      }),
    ).toEqual({
      label: "Transcription recovered",
      tone: "warn",
      detail: "transcription worker was not running after app restart",
    });

    expect(
      mapCommandJobState({
        id: "summary-circuit-review-1700000002000",
        kind: "Summary",
        meetingId: "circuit-review",
        state: "Retry",
        cancelRequested: false,
        startedAtMs: 1_700_000_002_000,
      }),
    ).toEqual({
      label: "Summary retryable",
      tone: "warn",
      detail: "Retry this summary job when you are ready.",
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
    expect(mapPermissionState("SystemAudioUnavailable")).toEqual({
      label: "System audio unavailable",
      tone: "blocked",
      detail: "Run the ScreenCaptureKit desktop backend and allow Screen Recording before recording.",
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
    expect(mapModelStatus({ kind: "untested", configuredPath: "/models/base.en.bin" })).toEqual({
      label: "Whisper path untested",
      tone: "blocked",
      detail: "Run Test path for the saved model file before transcription.",
    });
    expect(mapModelStatus({ kind: "unsupported", configuredPath: "/models/notes.txt" })).toEqual({
      label: "Whisper file unsupported",
      tone: "blocked",
      detail: "Choose a supported .bin or .gguf Whisper model file before transcription.",
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
    expect(mapExportState({ state: "exported", format: "markdown", path: "/tmp/circuit-review.md" })).toEqual({
      label: "Markdown exported",
      tone: "ready",
      detail: "/tmp/circuit-review.md",
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
    const workspaceControls = screen.getByLabelText("Workspace controls");
    expect(within(workspaceControls).getByText(`v${packageInfo.version}`)).toBeInTheDocument();
    expect(within(workspaceControls).queryByText("Recording unavailable")).not.toBeInTheDocument();
    expect(within(workspaceControls).queryByText("Whisper model missing")).not.toBeInTheDocument();
    expect(within(workspaceControls).queryByText("Transcription idle")).not.toBeInTheDocument();
    expect(within(workspaceControls).queryByText("Preview shell")).not.toBeInTheDocument();
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

  it("switches the workspace between dark and light themes", async () => {
    const user = userEvent.setup();
    render(<App />);

    expect(screen.getByRole("main")).toHaveAttribute("data-theme", "dark");
    expect(screen.getByRole("button", { name: "Switch to light mode" })).toHaveAttribute(
      "aria-pressed",
      "false",
    );

    await user.click(screen.getByRole("button", { name: "Switch to light mode" }));

    expect(screen.getByRole("main")).toHaveAttribute("data-theme", "light");
    expect(screen.getByRole("button", { name: "Switch to dark mode" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
  });

  it("renders editable local settings from the desktop snapshot", () => {
    const snapshot = connectedSnapshot({
      settings: {
        whisperModelPath: "/models/ggml-base.en.bin",
        ollamaBaseUrl: "http://127.0.0.1:11435",
        ollamaModel: "gemma4:31b",
        exportDirectory: null,
        rawAudioRetentionPolicy: "Retain",
      },
    });
    render(
      <App
        snapshot={snapshot}
        commandFacade={fakeCommandFacade()}
      />,
    );

    expect(screen.getByLabelText("Whisper model path")).toHaveValue("/models/ggml-base.en.bin");
    expect(screen.getByLabelText("Ollama base URL")).toHaveValue("http://127.0.0.1:11435");
    expect(screen.getByLabelText("Ollama model")).toHaveValue("gemma4:31b");
    expect(screen.getByRole("button", { name: "Test path" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "Save Whisper" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "Save analysis" })).toBeEnabled();
  });

  it("shows first-run readiness guidance for missing Whisper and unchecked Ollama without downloads", () => {
    const snapshot = {
      ...connectedSnapshot({
        model: {
          kind: "missing",
          configuredPath: "",
        },
        settings: {
          whisperModelPath: "",
          ollamaBaseUrl: "http://127.0.0.1:11434",
          ollamaModel: "qwen3.6:27b",
          exportDirectory: null,
        rawAudioRetentionPolicy: "Retain",
        },
      }),
      setupGuidance: {
        whisper: {
          state: "MissingPath",
          configuredPath: "",
          message: "No Whisper model path is configured.",
          setupGuidance: "Enter a local Whisper model path, save it, then use Test path.",
          compatibilityNote: "Readability does not prove model compatibility.",
          lastPathTest: null,
          lastSuccessfulTranscription: null,
        },
        ollama: {
          state: "ConfiguredNotChecked",
          baseUrl: "http://127.0.0.1:11434",
          model: "qwen3.6:27b",
          availability: "UnknownUntilTest",
          message: "Ollama is configured for a local loopback URL and model.",
          setupGuidance: "Start Ollama manually, install the selected local model if needed, then run Test Ollama.",
        },
      },
    } as never;

    render(<App snapshot={snapshot} commandFacade={fakeCommandFacade()} />);

    const readiness = screen.getByLabelText("Model readiness guidance");
    expect(within(readiness).getByText("No Whisper model path is configured.")).toBeInTheDocument();
    expect(within(readiness).getByText("Readability does not prove model compatibility.")).toBeInTheDocument();
    expect(within(readiness).getByText("Ollama availability unknown")).toBeInTheDocument();
    expect(within(readiness).getByText("http://127.0.0.1:11434 / qwen3.6:27b")).toBeInTheDocument();
    expect(readiness.textContent?.toLowerCase()).not.toContain("download");
    expect(readiness.textContent).not.toContain("SHA-256");
    expect(readiness.textContent).not.toContain("Installed models:");
    expect(readiness.textContent).not.toContain("ollama pull");
  });

  it("renders manual model setup options without saving, testing, pulling, or downloading", async () => {
    const user = userEvent.setup();
    const calls: string[] = [];
    const copiedCommands: string[] = [];
    const commandFacade = fakeCommandFacade({
      desktopSnapshot: async () => {
        calls.push("desktopSnapshot");
        return connectedSnapshot();
      },
      saveWhisperModelPath: async () => {
        calls.push("saveWhisperModelPath");
        return connectedSnapshot();
      },
      saveAnalysisSettings: async () => {
        calls.push("saveAnalysisSettings");
        return connectedSnapshot();
      },
      testWhisperModelPath: async () => {
        calls.push("testWhisperModelPath");
        return {
          state: "Valid",
          message: "Whisper model path is readable.",
          setupGuidance: "",
          fileSizeBytes: 16,
          sha256: "8b68af71d2eaaec61d5b4f50e330493cc0074323676962d9761cbc7c6810ba54",
        };
      },
      testOllamaConnection: async () => {
        calls.push("testOllamaConnection");
        return {
          state: "Available",
          message: "Ollama is reachable.",
          setupGuidance: "",
          selectedLocalModelTag: "qwen3.6:27b",
          installedLocalModels: ["qwen3.6:27b"],
          pullCommand: null,
        };
      },
    });

    render(
      <App
        snapshot={connectedSnapshot()}
        commandFacade={commandFacade}
        clipboardWriter={{
          writeText: async (text) => {
            copiedCommands.push(text);
          },
        }}
      />,
    );

    const setupOptions = screen.getByLabelText("Manual model setup options");
    expect(within(setupOptions).getByText("Local Whisper file")).toBeInTheDocument();
    expect(within(setupOptions).getByText("Managed downloads unavailable")).toBeInTheDocument();
    expect(within(setupOptions).getByText("Local Ollama models")).toBeInTheDocument();
    expect(within(setupOptions).getByText("Manual pulls only")).toBeInTheDocument();
    expect(within(setupOptions).getByText("ollama pull qwen3.6:27b")).toBeInTheDocument();
    expect(within(setupOptions).getByText("ollama pull gemma4:31b")).toBeInTheDocument();

    await user.click(within(setupOptions).getByRole("button", { name: "Copy pull command for qwen3.6:27b" }));

    expect(copiedCommands).toEqual(["ollama pull qwen3.6:27b"]);
    expect(screen.getByRole("status")).toHaveTextContent("Pull command copied.");
    expect(screen.getByRole("status")).toHaveTextContent("Pull command: ollama pull qwen3.6:27b");
    expect(calls).toEqual([]);

    const useButtons = within(setupOptions).getAllByRole("button", { name: "Use" });
    expect(useButtons[0]).toBeDisabled();
    await user.click(useButtons[1]);

    expect(screen.getByLabelText("Ollama model")).toHaveValue("gemma4:31b");
    expect(calls).toEqual([]);
  });

  it("surfaces pull-command copy failures through settings feedback without desktop commands", async () => {
    const user = userEvent.setup();
    const calls: string[] = [];
    const attemptedCommands: string[] = [];
    const commandFacade = fakeCommandFacade({
      desktopSnapshot: async () => {
        calls.push("desktopSnapshot");
        return connectedSnapshot();
      },
      saveAnalysisSettings: async () => {
        calls.push("saveAnalysisSettings");
        return connectedSnapshot();
      },
      testOllamaConnection: async () => {
        calls.push("testOllamaConnection");
        return {
          state: "Available",
          message: "Ollama is reachable.",
          setupGuidance: "",
          selectedLocalModelTag: "qwen3.6:27b",
          installedLocalModels: ["qwen3.6:27b"],
          pullCommand: null,
        };
      },
    });

    render(
      <App
        snapshot={connectedSnapshot()}
        commandFacade={commandFacade}
        clipboardWriter={{
          writeText: async (text) => {
            attemptedCommands.push(text);
            throw new Error("clipboard denied");
          },
        }}
      />,
    );

    await user.click(
      within(screen.getByLabelText("Manual model setup options")).getByRole("button", {
        name: "Copy pull command for gemma4:31b",
      }),
    );

    expect(attemptedCommands).toEqual(["ollama pull gemma4:31b"]);
    expect(screen.getByRole("status")).toHaveTextContent("Could not copy pull command: clipboard denied");
    expect(screen.getByRole("status")).toHaveTextContent("Pull command: ollama pull gemma4:31b");
    expect(calls).toEqual([]);
  });

  it("describes a readable Whisper path as readable but not compatibility-verified", () => {
    const snapshot = {
      ...connectedSnapshot({
        model: {
          kind: "untested",
          configuredPath: "/models/ggml-base.en.bin",
        },
        settings: {
          whisperModelPath: "/models/ggml-base.en.bin",
          ollamaBaseUrl: "http://127.0.0.1:11434",
          ollamaModel: "qwen3.6:27b",
          exportDirectory: null,
        rawAudioRetentionPolicy: "Retain",
        },
      }),
      setupGuidance: {
        whisper: {
          state: "ReadablePath",
          configuredPath: "/models/ggml-base.en.bin",
          message: "Whisper model path is readable; compatibility is not verified.",
          setupGuidance: "Use Test path for file evidence, then transcribe a sample to verify compatibility.",
          compatibilityNote: "Readability does not prove model compatibility.",
          lastPathTest: null,
          lastSuccessfulTranscription: null,
        },
        ollama: {
          state: "ConfiguredNotChecked",
          baseUrl: "http://127.0.0.1:11434",
          model: "qwen3.6:27b",
          availability: "UnknownUntilTest",
          message: "Ollama is configured for a local loopback URL and model.",
          setupGuidance: "Start Ollama manually, install the selected local model if needed, then run Test Ollama.",
        },
      },
    } as never;

    render(<App snapshot={snapshot} commandFacade={fakeCommandFacade()} />);

    const readiness = screen.getByLabelText("Model readiness guidance");
    expect(within(readiness).getByText("Whisper path readable")).toBeInTheDocument();
    expect(
      within(readiness).getByText("Whisper model path is readable; compatibility is not verified."),
    ).toBeInTheDocument();
    expect(within(readiness).getByText("Readability does not prove model compatibility.")).toBeInTheDocument();
    expect(readiness.textContent).not.toContain("SHA-256");
  });

  it("shows matching last explicit setup test evidence without claiming current availability", () => {
    const snapshot = {
      ...connectedSnapshot({
        model: {
          kind: "ready",
          configuredPath: "/models/ggml-base.en.bin",
        },
        settings: {
          whisperModelPath: "/models/ggml-base.en.bin",
          ollamaBaseUrl: "http://127.0.0.1:11434",
          ollamaModel: "qwen3.6:27b",
          exportDirectory: null,
          rawAudioRetentionPolicy: "Retain",
        },
      }),
      setupGuidance: {
        whisper: {
          state: "ReadablePath",
          configuredPath: "/models/ggml-base.en.bin",
          message: "Whisper model path is readable; compatibility is not verified.",
          setupGuidance: "Use Test path for file evidence, then transcribe a sample to verify compatibility.",
          compatibilityNote: "Readability does not prove model compatibility.",
          lastPathTest: {
            testedPath: "/models/ggml-base.en.bin",
            testedAtMs: 1_700_000_001_000,
            state: "Valid",
            fileSizeBytes: 16,
            sha256: "8b68af71d2eaaec61d5b4f50e330493cc0074323676962d9761cbc7c6810ba54",
            failureDetail: null,
          },
          lastSuccessfulTranscription: null,
        },
        ollama: {
          state: "ConfiguredNotChecked",
          baseUrl: "http://127.0.0.1:11434",
          model: "qwen3.6:27b",
          availability: "AvailableAtLastTest",
          message: "Last explicit Test Ollama reached qwen3.6:27b; summaries were available at that test.",
          setupGuidance:
            "Availability is not checked in the background. Run Test Ollama again after changing Ollama, models, or the base URL.",
          lastConnectionTest: {
            baseUrl: "http://127.0.0.1:11434",
            requestedModel: "qwen3.6:27b",
            testedAtMs: 1_700_000_002_000,
            state: "Available",
            selectedLocalModelTag: "qwen3.6:27b",
            installedLocalModels: ["gemma4:31b", "qwen3.6:27b"],
            pullCommand: null,
            failureDetail: null,
          },
        },
      },
    } as never;

    render(<App snapshot={snapshot} commandFacade={fakeCommandFacade()} />);

    const readiness = screen.getByLabelText("Model readiness guidance");
    expect(within(readiness).getByText("Ollama available at last test")).toBeInTheDocument();
    expect(
      within(readiness).getByText("Last explicit Test path: Valid at 2023-11-14T22:13:21.000Z"),
    ).toBeInTheDocument();
    expect(within(readiness).getByText("Size: 16 bytes")).toBeInTheDocument();
    expect(
      within(readiness).getByText("SHA-256: 8b68af71d2eaaec61d5b4f50e330493cc0074323676962d9761cbc7c6810ba54"),
    ).toBeInTheDocument();
    expect(
      within(readiness).getByText("Last explicit Test Ollama: Available at 2023-11-14T22:13:22.000Z"),
    ).toBeInTheDocument();
    expect(within(readiness).getByText("Observed models: gemma4:31b, qwen3.6:27b")).toBeInTheDocument();
    expect(within(readiness).getByText("Last explicit observation, not current availability.")).toBeInTheDocument();
    expect(readiness.textContent).not.toContain("compatibility is verified");
    expect(readiness.textContent?.toLowerCase()).not.toContain("is compatible");
  });

  it("shows last successful Whisper transcription evidence separately from path evidence", () => {
    const setupGuidance = whisperSetupGuidanceForPath(
      "/models/ggml-base.en.bin",
      validWhisperPathTestEvidence("/models/ggml-base.en.bin"),
    );
    const snapshot = connectedSnapshot({
      model: {
        kind: "ready",
        configuredPath: "/models/ggml-base.en.bin",
      },
      setupGuidance: {
        ...setupGuidance,
        whisper: {
          ...setupGuidance.whisper,
          message: "Whisper model path is readable and has completed transcription before.",
          compatibilityNote:
            "Last successful transcription is historical evidence for this local path, not a background compatibility check.",
          lastSuccessfulTranscription: {
            modelPath: "/models/ggml-base.en.bin",
            usedAtMs: 1_700_000_005_000,
            provider: "local-whisper",
            modelName: "ggml-base.en.bin",
            meetingId: "meeting-1",
            modelRunId: "run-1",
            transcriptVersionId: "version-1",
            segmentCount: 2,
            fileSizeBytes: 16,
            modifiedAtMs: 1_700_000_004_000,
          },
        },
      },
      settings: {
        whisperModelPath: "/models/ggml-base.en.bin",
        ollamaBaseUrl: "http://127.0.0.1:11434",
        ollamaModel: "qwen3.6:27b",
        exportDirectory: null,
        rawAudioRetentionPolicy: "Retain",
      },
    });

    render(<App snapshot={snapshot} commandFacade={fakeCommandFacade()} />);

    const readiness = screen.getByLabelText("Model readiness guidance");
    expect(within(readiness).getByText("Last explicit Test path: Valid at 2023-11-14T22:13:21.000Z")).toBeInTheDocument();
    expect(within(readiness).getByText("Last successful transcription at 2023-11-14T22:13:25.000Z")).toBeInTheDocument();
    expect(within(readiness).getByText("Provider: local-whisper")).toBeInTheDocument();
    expect(within(readiness).getByText("Meeting: meeting-1")).toBeInTheDocument();
    expect(within(readiness).getByText("Model run: run-1")).toBeInTheDocument();
    expect(within(readiness).getByText("Transcript version: version-1")).toBeInTheDocument();
    expect(within(readiness).getByText("Transcript: 2 segments")).toBeInTheDocument();
    expect(within(readiness).getByText("Model modified: 2023-11-14T22:13:24.000Z")).toBeInTheDocument();
    expect(
      within(readiness).getByText(
        "Last successful transcription is historical evidence for this local path, not a background compatibility check.",
      ),
    ).toBeInTheDocument();
  });

  it("shows missing Ollama model evidence with the deterministic pull command", async () => {
    const user = userEvent.setup();
    const calls: string[] = [];
    const copiedCommands: string[] = [];
    const snapshot = {
      ...connectedSnapshot({
        model: {
          kind: "ready",
          configuredPath: "/models/ggml-base.en.bin",
        },
        settings: {
          whisperModelPath: "/models/ggml-base.en.bin",
          ollamaBaseUrl: "http://127.0.0.1:11434",
          ollamaModel: "qwen3.6:27b",
          exportDirectory: null,
          rawAudioRetentionPolicy: "Retain",
        },
      }),
      setupGuidance: {
        whisper: {
          state: "ReadablePath",
          configuredPath: "/models/ggml-base.en.bin",
          message: "Whisper model path is readable; compatibility is not verified.",
          setupGuidance: "Use Test path for file evidence, then transcribe a sample to verify compatibility.",
          compatibilityNote: "Readability does not prove model compatibility.",
          lastPathTest: validWhisperPathTestEvidence("/models/ggml-base.en.bin"),
          lastSuccessfulTranscription: null,
        },
        ollama: {
          state: "ConfiguredNotChecked",
          baseUrl: "http://127.0.0.1:11434",
          model: "qwen3.6:27b",
          availability: "MissingModelAtLastTest",
          message:
            "Last explicit Test Ollama reached Ollama, but qwen3.6:27b was missing. Summaries are unavailable until the selected local model is installed.",
          setupGuidance:
            "Run `ollama pull qwen3.6:27b`, then run Test Ollama again. Availability is not checked in the background.",
          lastConnectionTest: {
            baseUrl: "http://127.0.0.1:11434",
            requestedModel: "qwen3.6:27b",
            testedAtMs: 1_700_000_003_000,
            state: "Unavailable",
            selectedLocalModelTag: "qwen3.6:27b",
            installedLocalModels: ["gemma4:31b"],
            pullCommand: "ollama pull qwen3.6:27b",
            failureDetail: "Ollama is reachable, but qwen3.6:27b is not installed.",
          },
        },
      },
    } as never;

    render(
      <App
        snapshot={snapshot}
        commandFacade={fakeCommandFacade({
          desktopSnapshot: async () => {
            calls.push("desktopSnapshot");
            return connectedSnapshot();
          },
          saveAnalysisSettings: async () => {
            calls.push("saveAnalysisSettings");
            return connectedSnapshot();
          },
          testOllamaConnection: async () => {
            calls.push("testOllamaConnection");
            return {
              state: "Available",
              message: "Ollama is reachable.",
              setupGuidance: "",
              selectedLocalModelTag: "qwen3.6:27b",
              installedLocalModels: ["qwen3.6:27b"],
              pullCommand: null,
            };
          },
        })}
        clipboardWriter={{
          writeText: async (text) => {
            copiedCommands.push(text);
          },
        }}
      />,
    );

    const readiness = screen.getByLabelText("Model readiness guidance");
    expect(within(readiness).getByText("Ollama model missing")).toBeInTheDocument();
    expect(within(readiness).getByText("Pull command: ollama pull qwen3.6:27b")).toBeInTheDocument();
    expect(within(readiness).getByText("Observed models: gemma4:31b")).toBeInTheDocument();
    expect(readiness.textContent).toContain("Summaries are unavailable");
    expect(readiness.textContent).toContain("Last explicit observation, not current availability.");

    await user.click(within(readiness).getByRole("button", { name: "Copy pull command for qwen3.6:27b" }));

    expect(copiedCommands).toEqual(["ollama pull qwen3.6:27b"]);
    expect(screen.getByRole("status")).toHaveTextContent("Pull command copied.");
    expect(screen.getByRole("status")).toHaveTextContent("Pull command: ollama pull qwen3.6:27b");
    expect(calls).toEqual([]);
  });

  it("shows unavailable Ollama test evidence without suggesting a model pull", () => {
    const snapshot = {
      ...connectedSnapshot({
        settings: {
          whisperModelPath: "/models/ggml-base.en.bin",
          ollamaBaseUrl: "http://127.0.0.1:11434",
          ollamaModel: "qwen3.6:27b",
          exportDirectory: null,
          rawAudioRetentionPolicy: "Retain",
        },
      }),
      setupGuidance: {
        whisper: {
          state: "ReadablePath",
          configuredPath: "/models/ggml-base.en.bin",
          message: "Whisper model path is readable; compatibility is not verified.",
          setupGuidance: "Use Test path for file evidence, then transcribe a sample to verify compatibility.",
          compatibilityNote: "Readability does not prove model compatibility.",
          lastPathTest: validWhisperPathTestEvidence("/models/ggml-base.en.bin"),
          lastSuccessfulTranscription: null,
        },
        ollama: {
          state: "ConfiguredNotChecked",
          baseUrl: "http://127.0.0.1:11434",
          model: "qwen3.6:27b",
          availability: "UnavailableAtLastTest",
          message: "Last explicit Test Ollama could not confirm local summary availability.",
          setupGuidance:
            "Ollama is unavailable: connection refused. Start Ollama with `ollama serve`, verify the local base URL, then run Test Ollama again. Availability is not checked in the background.",
          lastConnectionTest: {
            baseUrl: "http://127.0.0.1:11434",
            requestedModel: "qwen3.6:27b",
            testedAtMs: 1_700_000_004_000,
            state: "Unavailable",
            selectedLocalModelTag: "qwen3.6:27b",
            installedLocalModels: null,
            pullCommand: null,
            failureDetail: "Ollama is unavailable: connection refused.",
          },
        },
      },
    } as never;

    render(<App snapshot={snapshot} commandFacade={fakeCommandFacade()} />);

    const readiness = screen.getByLabelText("Model readiness guidance");
    expect(within(readiness).getByText("Summaries unavailable")).toBeInTheDocument();
    expect(within(readiness).getByText("Ollama is unavailable: connection refused.")).toBeInTheDocument();
    expect(readiness.textContent).not.toContain("Pull command:");
    expect(readiness.textContent).toContain("Last explicit observation, not current availability.");
  });

  it("shows read-only calendar context without disabling manual recording", () => {
    const snapshot = connectedSnapshot({
      recording: {
        ...getMockDesktopSnapshot().recording,
        meeting_id: "",
        recording_id: null,
        state: "Idle",
        permission_state: "Ready",
        recovery_action: "Start a desktop recording to create private microphone and system audio WAV artifacts.",
      },
      calendarContext: {
        source: "AppleCalendar",
        permissionState: "NotRequested",
        availabilityState: "PermissionRequired",
        message: "Apple Calendar permission has not been requested.",
        setupGuidance:
          "Use Request calendar access when you want Curiosity to read upcoming local Calendar events. Calendar events never start recordings automatically.",
        upcomingEvents: [],
        autoStartEnabled: false,
      },
    });

    render(<App snapshot={snapshot} commandFacade={fakeCommandFacade()} />);

    const calendarContext = screen.getByLabelText("Calendar context");
    expect(within(calendarContext).getByText("Calendar permission needed")).toBeInTheDocument();
    expect(within(calendarContext).getByText("Apple Calendar permission has not been requested.")).toBeInTheDocument();
    expect(within(calendarContext).getByText("No upcoming calendar events loaded.")).toBeInTheDocument();
    expect(within(calendarContext).getByText("Auto-start disabled.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Request calendar access" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "Start recording" })).toBeEnabled();
  });

  it("requests Apple Calendar access only from the explicit calendar button", async () => {
    const user = userEvent.setup();
    const initial = connectedSnapshot({
      calendarContext: {
        ...getMockDesktopSnapshot().calendarContext,
        permissionState: "NotRequested",
        availabilityState: "PermissionRequired",
        message: "Apple Calendar permission has not been requested.",
      },
    });
    const granted = {
      ...initial,
      calendarContext: {
        ...initial.calendarContext,
        permissionState: "Granted" as const,
        availabilityState: "Ready" as const,
        message: "Apple Calendar access is granted; no upcoming events found in the next 24 hours.",
      },
    };
    const requestAppleCalendarAccess = vi.fn(async () => granted);

    render(
      <App
        snapshot={initial}
        commandFacade={fakeCommandFacade({
          requestAppleCalendarAccess,
        })}
      />,
    );

    await user.click(screen.getByRole("button", { name: "Request calendar access" }));

    expect(requestAppleCalendarAccess).toHaveBeenCalledTimes(1);
    expect(screen.getByLabelText("Calendar context")).toHaveTextContent(
      "Apple Calendar access is granted; no upcoming events found in the next 24 hours.",
    );
    expect(screen.queryByRole("button", { name: "Request calendar access" })).not.toBeInTheDocument();
  });

  it("renders loaded Apple Calendar events as read-only safety context", () => {
    const snapshot = connectedSnapshot({
      recording: {
        ...getMockDesktopSnapshot().recording,
        meeting_id: "",
        recording_id: null,
        state: "Idle",
        permission_state: "Ready",
        recovery_action: "Start a desktop recording to create private microphone and system audio WAV artifacts.",
      },
      calendarContext: {
        source: "AppleCalendar",
        permissionState: "Granted",
        availabilityState: "Ready",
        message: "Apple Calendar access is granted; 2 upcoming events loaded for manual review.",
        setupGuidance:
          "Upcoming local events are read-only and not stored. Manual attachment remains disabled in this slice, and calendar events never start recordings automatically.",
        upcomingEvents: [
          {
            id: "event-1",
            title: "Design Review",
            calendarTitle: "Work",
            startsAtMs: Date.UTC(2026, 6, 8, 9, 0),
            endsAtMs: Date.UTC(2026, 6, 8, 10, 0),
            isAllDay: false,
            isRecurring: false,
            privacy: "Unknown",
            overlapState: "Overlapping",
            attachable: false,
            safetyNote: "Overlaps another event; attachment is disabled until ambiguity handling is implemented.",
          },
          {
            id: "event-2",
            title: "Private Planning",
            calendarTitle: "Leadership",
            startsAtMs: Date.UTC(2026, 6, 8, 9, 30),
            endsAtMs: Date.UTC(2026, 6, 8, 10, 30),
            isAllDay: false,
            isRecurring: true,
            privacy: "Private",
            overlapState: "Overlapping",
            attachable: false,
            safetyNote: "Recurring event; attachment is disabled until recurrence handling is implemented.",
          },
        ],
        autoStartEnabled: false,
      },
    });

    render(<App snapshot={snapshot} commandFacade={fakeCommandFacade()} />);

    const calendarContext = screen.getByLabelText("Calendar context");
    expect(within(calendarContext).getByText("Design Review")).toBeInTheDocument();
    expect(within(calendarContext).getByText(/Work.*Unknown privacy.*Overlapping/)).toBeInTheDocument();
    expect(
      within(calendarContext).getByText(
        "Overlaps another event; attachment is disabled until ambiguity handling is implemented.",
      ),
    ).toBeInTheDocument();
    expect(within(calendarContext).getByText("Private Planning")).toBeInTheDocument();
    expect(
      within(calendarContext).getByText(/Leadership.*Private privacy.*Overlapping.*Recurring/),
    ).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /attach/i })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Start recording" })).toBeEnabled();
  });

  it("attaches an explicitly confirmed safe Calendar event to the selected meeting", async () => {
    const user = userEvent.setup();
    const initial = connectedSnapshot({
      calendarContext: {
        source: "AppleCalendar",
        permissionState: "Granted",
        availabilityState: "Ready",
        message: "Apple Calendar access is granted; 1 upcoming events loaded for manual review.",
        setupGuidance:
          "Upcoming local events are read-only until you explicitly attach one as meeting context.",
        upcomingEvents: [
          {
            id: "event-1",
            title: "Design Review",
            calendarTitle: "Work",
            startsAtMs: Date.UTC(2026, 6, 8, 9, 0),
            endsAtMs: Date.UTC(2026, 6, 8, 10, 0),
            isAllDay: false,
            isRecurring: false,
            privacy: "Unknown",
            overlapState: "None",
            attachable: true,
            safetyNote:
              "Privacy classification is unavailable from EventKit; confirm this event title is safe before attaching.",
          },
        ],
        autoStartEnabled: false,
      },
    });
    const attached = connectedSnapshot({
      meetings: [
        {
          ...initial.meetings[0],
          calendarAttachment: {
            source: "AppleCalendar",
            eventId: "event-1",
            eventTitle: "Design Review",
            calendarTitle: "Work",
            startsAtMs: Date.UTC(2026, 6, 8, 9, 0),
            endsAtMs: Date.UTC(2026, 6, 8, 10, 0),
            privacy: "Unknown",
            privacyConfirmed: true,
            attachedAtMs: Date.UTC(2026, 6, 8, 8, 45),
          },
        },
      ],
      selectedMeetingId: initial.selectedMeetingId,
      calendarContext: initial.calendarContext,
    });
    const attachCalendarEventContext = vi.fn(async () => attached);

    render(
      <App
        snapshot={initial}
        commandFacade={fakeCommandFacade({
          attachCalendarEventContext,
        })}
      />,
    );

    await user.click(screen.getByRole("button", { name: "Confirm privacy and attach" }));

    expect(attachCalendarEventContext).toHaveBeenCalledWith({
      meetingId: "circuit-review",
      eventId: "event-1",
      privacyConfirmed: true,
    });
    expect(screen.getAllByText(/Design Review/).length).toBeGreaterThan(0);
    expect(screen.getByText(/Unknown privacy confirmed/)).toBeInTheDocument();
  });

  it("keeps local settings controls usable when desktop commands are unavailable", async () => {
    const user = userEvent.setup();

    render(<App snapshot={getMockDesktopSnapshot()} />);

    const whisperPath = screen.getByLabelText("Whisper model path");
    const ollamaBaseUrl = screen.getByLabelText("Ollama base URL");
    const ollamaModel = screen.getByLabelText("Ollama model");

    await user.type(whisperPath, "/models/ggml-base.en.bin");
    await user.clear(ollamaBaseUrl);
    await user.type(ollamaBaseUrl, "http://127.0.0.1:11435");
    await user.clear(ollamaModel);
    await user.type(ollamaModel, "gemma4:31b");

    expect(whisperPath).toHaveValue("/models/ggml-base.en.bin");
    expect(ollamaBaseUrl).toHaveValue("http://127.0.0.1:11435");
    expect(ollamaModel).toHaveValue("gemma4:31b");
    expect(screen.getByRole("button", { name: "Test path" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "Save Whisper" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "Test Ollama" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "Save analysis" })).toBeEnabled();

    await user.click(screen.getByRole("button", { name: "Save analysis" }));

    expect(screen.getByRole("status")).toHaveTextContent(
      "Preview shell: backend command wiring is not connected in this browser/dev fixture.",
    );
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

    expect(screen.getByText("Preview shell: backend command wiring is not connected in this browser/dev fixture.")).toBeInTheDocument();
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

  it("shows selected-meeting raw-audio retention and local processing privacy state", () => {
    render(<App />);

    const privacyState = screen.getByLabelText("Meeting privacy data state");
    expect(within(privacyState).getByText("Raw audio retained")).toBeInTheDocument();
    expect(within(privacyState).getByText("Raw audio retained in private app storage.")).toBeInTheDocument();
    expect(within(privacyState).getByText("Stayed local")).toBeInTheDocument();
    expect(within(privacyState).getByText("No hosted processing recorded for this meeting.")).toBeInTheDocument();
  });

  it("shows when selected-meeting transcript or summary processing may have left the device", () => {
    const snapshot = getMockDesktopSnapshot();
    render(
      <App
        snapshot={{
          ...snapshot,
          meetings: snapshot.meetings.map((meeting) =>
            meeting.id === "circuit-review"
              ? {
                  ...meeting,
                  privacy: {
                    ...meeting.privacy,
                    localOnly: false,
                  },
                }
              : meeting,
          ),
        }}
      />,
    );

    const privacyState = screen.getByLabelText("Meeting privacy data state");
    expect(within(privacyState).getByText("Hosted processing used")).toBeInTheDocument();
    expect(within(privacyState).getByText("Transcript/summary data may have left this device.")).toBeInTheDocument();
  });

  it("does not imply connected recording support when desktop commands are read-only", () => {
    render(
      <App
        snapshot={{
          ...getMockDesktopSnapshot(),
          commandSurface: {
            ready: true,
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
    expect(
      within(screen.getByLabelText("Recording controls and status")).queryByText(
        "Raw audio retained in private app storage.",
      ),
    ).not.toBeInTheDocument();
    expect(screen.getByText("Recording commands are not wired into the desktop shell yet.")).toBeInTheDocument();
  });

  it("shows transcription failure returned by desktop commands", () => {
    render(
      <App
        snapshot={{
          ...getMockDesktopSnapshot(),
          commandSurface: {
            ready: true,
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
    const calls: Array<{ method: string; args?: unknown }> = [];
    const commandFacade = fakeCommandFacade({
      searchMeetings: async (args) => {
        calls.push({ method: "searchMeetings", args });
        return [{ meeting_id: "design-standup", title: "Design Standup" }] as never;
      },
    });

    render(<App snapshot={initial} commandFacade={commandFacade} />);

    await user.type(screen.getByLabelText("Search meetings"), "layout");

    await waitFor(() =>
      expect(calls).toContainEqual({
        method: "searchMeetings",
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
      commandSurface: { ready: true, detail: "Connected to local desktop commands." },
      selectedMeetingId: "design-standup",
    };
    const { rerender } = render(<App snapshot={{ ...loading, loading: true }} />);

    rerender(<App snapshot={loaded} />);

    expect(screen.getByRole("button", { name: /Design Standup/ })).toHaveAttribute("aria-current", "page");
    expect(screen.getByRole("heading", { name: "Design Standup" })).toBeInTheDocument();
  });

  it("starts desktop recording with the optional title and replaces the snapshot", async () => {
    const user = userEvent.setup();
    const calls: Array<{ method: string; args?: unknown }> = [];
    const initial = connectedSnapshot({
      recording: {
        ...getMockDesktopSnapshot().recording,
        state: "Complete",
        recovery_action: "Previous local desktop WAV artifacts are saved.",
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
    const commandFacade = fakeCommandFacade({
      startRecording: async (args) => {
        calls.push({ method: "startRecording", args });
        return returned;
      },
    });

    render(<App snapshot={initial} commandFacade={commandFacade} />);

    await user.type(screen.getByLabelText("Recording title"), "MVP sync");
    await user.click(screen.getByRole("button", { name: "Start recording" }));

    expect(calls).toEqual([{ method: "startRecording", args: { title: "MVP sync" } }]);
    expect(screen.getByRole("heading", { name: "Design Standup" })).toBeInTheDocument();
    expect(screen.getAllByText("Recording").length).toBeGreaterThan(0);
    expect(screen.getAllByText("meetings/design-standup/audio").length).toBeGreaterThan(0);
  });

  it("imports a user-provided WAV path with the optional title and replaces the snapshot", async () => {
    const user = userEvent.setup();
    const calls: Array<{ method: string; args?: unknown }> = [];
    const initial = connectedSnapshot({
      recording: {
        ...getMockDesktopSnapshot().recording,
        state: "Complete",
        recovery_action: "Previous local desktop WAV artifacts are saved.",
      },
    });
    const returned = connectedSnapshot({
      selectedMeetingId: "imported-call",
      meetings: [
        {
          ...getMockDesktopSnapshot().meetings[0],
          id: "imported-call",
          title: "Imported Call",
        },
      ],
      recording: {
        ...initial.recording,
        meeting_id: "imported-call",
        recording_id: "recording-imported-call",
        state: "Complete",
        recovery_action: "Imported local WAV into private app storage.",
        storage_location: { app_private_path: "meetings/imported-call/audio" },
      },
    });
    const commandFacade = fakeCommandFacade({
      importAudioFile: async (args) => {
        calls.push({ method: "importAudioFile", args });
        return returned;
      },
    });

    render(<App snapshot={initial} commandFacade={commandFacade} />);

    await user.type(screen.getByLabelText("Recording title"), "Customer call");
    await user.type(screen.getByLabelText("WAV source path"), "/Users/adrian/imports/customer-call.wav");
    await user.click(screen.getByRole("button", { name: "Import WAV" }));

    expect(calls).toEqual([
      {
        method: "importAudioFile",
        args: {
          sourcePath: "/Users/adrian/imports/customer-call.wav",
          title: "Customer call",
        },
      },
    ]);
    expect(screen.getByRole("heading", { name: "Imported Call" })).toBeInTheDocument();
    expect(screen.getAllByText("Imported local WAV into private app storage.").length).toBeGreaterThan(0);
    expect(screen.getAllByText("meetings/imported-call/audio").length).toBeGreaterThan(0);
    expect(screen.getByLabelText("WAV source path")).toHaveValue("");
  });

  it("chooses a WAV path with the native picker and imports the selected path", async () => {
    const user = userEvent.setup();
    const calls: Array<{ method: string; args?: unknown }> = [];
    const initial = connectedSnapshot({
      recording: {
        ...getMockDesktopSnapshot().recording,
        state: "Complete",
        recovery_action: "Previous local desktop WAV artifacts are saved.",
      },
    });
    const commandFacade = fakeCommandFacade({
      importAudioFile: async (args) => {
        calls.push({ method: "importAudioFile", args });
        return connectedSnapshot();
      },
    });

    render(
      <App
        snapshot={initial}
        commandFacade={commandFacade}
        filePicker={{
          chooseImportWavPath: async () => "/Users/adrian/imports/chosen.wav",
        }}
      />,
    );

    await user.click(screen.getByRole("button", { name: "Choose WAV" }));

    expect(screen.getByLabelText("WAV source path")).toHaveValue("/Users/adrian/imports/chosen.wav");

    await user.click(screen.getByRole("button", { name: "Import WAV" }));

    expect(calls).toEqual([
      {
        method: "importAudioFile",
        args: {
          sourcePath: "/Users/adrian/imports/chosen.wav",
        },
      },
    ]);
  });

  it("uses a scoped single-file WAV native picker by default", async () => {
    const user = userEvent.setup();
    const initial = connectedSnapshot({
      recording: {
        ...getMockDesktopSnapshot().recording,
        state: "Complete",
        recovery_action: "Previous local desktop WAV artifacts are saved.",
      },
    });
    dialogOpen.mockResolvedValue("/Users/adrian/imports/default-picker.wav");

    render(<App snapshot={initial} commandFacade={fakeCommandFacade()} />);

    await user.click(screen.getByRole("button", { name: "Choose WAV" }));

    expect(dialogOpen).toHaveBeenCalledWith({
      title: "Choose WAV audio file",
      multiple: false,
      directory: false,
      fileAccessMode: "scoped",
      filters: [
        {
          name: "WAV audio",
          extensions: ["wav"],
        },
      ],
    });
    expect(screen.getByLabelText("WAV source path")).toHaveValue("/Users/adrian/imports/default-picker.wav");
  });

  it("uses a scoped single-file Whisper model native picker by default", async () => {
    const user = userEvent.setup();
    const initial = connectedSnapshot({
      settings: {
        whisperModelPath: "",
        ollamaBaseUrl: "http://127.0.0.1:11434",
        ollamaModel: "qwen3.6:27b",
        exportDirectory: null,
        rawAudioRetentionPolicy: "Retain",
      },
    });
    dialogOpen.mockResolvedValue("/Users/adrian/models/ggml-base.en.bin");

    render(<App snapshot={initial} commandFacade={fakeCommandFacade()} />);

    await user.click(screen.getByRole("button", { name: "Choose model" }));

    expect(dialogOpen).toHaveBeenCalledWith({
      title: "Choose Whisper model file",
      multiple: false,
      directory: false,
      fileAccessMode: "scoped",
      filters: [
        {
          name: "Whisper model",
          extensions: ["bin", "gguf"],
        },
      ],
    });
    expect(screen.getByLabelText("Whisper model path")).toHaveValue("/Users/adrian/models/ggml-base.en.bin");
  });

  it("preserves a typed WAV path when the native picker is canceled", async () => {
    const user = userEvent.setup();
    const initial = connectedSnapshot({
      recording: {
        ...getMockDesktopSnapshot().recording,
        state: "Complete",
        recovery_action: "Previous local desktop WAV artifacts are saved.",
      },
    });

    render(
      <App
        snapshot={initial}
        commandFacade={fakeCommandFacade()}
        filePicker={{
          chooseImportWavPath: async () => null,
        }}
      />,
    );

    await user.type(screen.getByLabelText("WAV source path"), "/Users/adrian/imports/typed.wav");
    await user.click(screen.getByRole("button", { name: "Choose WAV" }));

    expect(screen.getByLabelText("WAV source path")).toHaveValue("/Users/adrian/imports/typed.wav");
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("preserves a typed WAV path and reports native picker errors", async () => {
    const user = userEvent.setup();
    const initial = connectedSnapshot({
      recording: {
        ...getMockDesktopSnapshot().recording,
        state: "Complete",
        recovery_action: "Previous local desktop WAV artifacts are saved.",
      },
    });

    render(
      <App
        snapshot={initial}
        commandFacade={fakeCommandFacade()}
        filePicker={{
          chooseImportWavPath: async () => {
            throw new Error("native dialog failed");
          },
        }}
      />,
    );

    await user.type(screen.getByLabelText("WAV source path"), "/Users/adrian/imports/typed.wav");
    await user.click(screen.getByRole("button", { name: "Choose WAV" }));

    expect(screen.getByLabelText("WAV source path")).toHaveValue("/Users/adrian/imports/typed.wav");
    expect(screen.getByRole("alert")).toHaveTextContent("native dialog failed");
  });

  it("disables native WAV picking when commands are unavailable, recording is active, or another command is busy", async () => {
    const user = userEvent.setup();
    const unavailable = connectedSnapshot({
      commandSurface: {
        ready: false,
        detail: "Desktop command surface is unavailable.",
      },
    });
    const activeRecording = connectedSnapshot({
      recording: {
        ...getMockDesktopSnapshot().recording,
        state: "Recording",
        permission_state: "Ready",
      },
    });
    const initial = connectedSnapshot({
      recording: {
        ...getMockDesktopSnapshot().recording,
        state: "Complete",
        recovery_action: "Previous local desktop WAV artifacts are saved.",
      },
    });
    let finishCommand!: () => void;
    const returnedAfterStart = connectedSnapshot({
      recording: {
        ...getMockDesktopSnapshot().recording,
        state: "Complete",
        recovery_action: "Previous local desktop WAV artifacts are saved.",
      },
    });
    const pendingSnapshot = new Promise<ReturnType<typeof connectedSnapshot>>((resolve) => {
      finishCommand = () => resolve(returnedAfterStart);
    });
    const commandFacade = fakeCommandFacade({
      startRecording: async () => pendingSnapshot,
    });

    const { rerender } = render(
      <App
        snapshot={unavailable}
        commandFacade={fakeCommandFacade()}
        filePicker={{ chooseImportWavPath: async () => "/Users/adrian/imports/chosen.wav" }}
      />,
    );

    expect(screen.getByRole("button", { name: "Choose WAV" })).toBeDisabled();

    rerender(
      <App
        snapshot={activeRecording}
        commandFacade={fakeCommandFacade()}
        filePicker={{ chooseImportWavPath: async () => "/Users/adrian/imports/chosen.wav" }}
      />,
    );
    expect(screen.getByRole("button", { name: "Choose WAV" })).toBeDisabled();

    rerender(
      <App
        snapshot={initial}
        commandFacade={commandFacade}
        filePicker={{ chooseImportWavPath: async () => "/Users/adrian/imports/chosen.wav" }}
      />,
    );
    await user.click(screen.getByRole("button", { name: "Start recording" }));
    expect(screen.getByRole("button", { name: "Choose WAV" })).toBeDisabled();

    finishCommand();
    expect(await screen.findByRole("button", { name: "Choose WAV" })).toBeEnabled();
  });

  it("imports a WAV as the first meeting from an empty workspace", async () => {
    const user = userEvent.setup();
    const calls: Array<{ method: string; args?: unknown }> = [];
    const initial = connectedSnapshot({
      meetings: [],
      selectedMeetingId: null,
      recording: {
        ...getMockDesktopSnapshot().recording,
        meeting_id: "",
        recording_id: null,
        state: "Idle",
        recovery_action: "Start a desktop recording to create private microphone and system audio WAV artifacts.",
      },
    });
    const returned = connectedSnapshot({
      selectedMeetingId: "first-import",
      meetings: [
        {
          ...getMockDesktopSnapshot().meetings[0],
          id: "first-import",
          title: "First Import",
        },
      ],
      recording: {
        ...initial.recording,
        meeting_id: "first-import",
        recording_id: "recording-first-import",
        state: "Complete",
        recovery_action: "Imported local WAV into private app storage.",
        storage_location: { app_private_path: "meetings/first-import/audio" },
      },
    });
    const commandFacade = fakeCommandFacade({
      importAudioFile: async (args) => {
        calls.push({ method: "importAudioFile", args });
        return returned;
      },
    });

    render(<App snapshot={initial} commandFacade={commandFacade} />);

    await user.type(screen.getByLabelText("Recording title"), "First import");
    await user.type(screen.getByLabelText("WAV source path"), "/Users/adrian/imports/first.wav");
    await user.click(screen.getByRole("button", { name: "Import WAV" }));

    expect(calls).toEqual([
      {
        method: "importAudioFile",
        args: {
          sourcePath: "/Users/adrian/imports/first.wav",
          title: "First import",
        },
      },
    ]);
    expect(screen.getByRole("heading", { name: "First Import" })).toBeInTheDocument();
  });

  it("preserves the typed WAV path after unrelated successful commands", async () => {
    const user = userEvent.setup();
    const initial = connectedSnapshot({
      recording: {
        ...getMockDesktopSnapshot().recording,
        state: "Complete",
        recovery_action: "Previous local desktop WAV artifacts are saved.",
      },
    });
    const commandFacade = fakeCommandFacade({
      startRecording: async () => connectedSnapshot(),
    });

    render(<App snapshot={initial} commandFacade={commandFacade} />);

    await user.type(screen.getByLabelText("WAV source path"), "/Users/adrian/imports/keep.wav");
    await user.click(screen.getByRole("button", { name: "Start recording" }));

    expect(screen.getByLabelText("WAV source path")).toHaveValue("/Users/adrian/imports/keep.wav");
  });

  it("preserves the typed WAV path after a failed import", async () => {
    const user = userEvent.setup();
    const initial = connectedSnapshot({
      recording: {
        ...getMockDesktopSnapshot().recording,
        state: "Complete",
        recovery_action: "Previous local desktop WAV artifacts are saved.",
      },
    });
    const commandFacade = fakeCommandFacade({
      importAudioFile: async () => {
        throw new Error("WAV source file has an unsupported WAV header.");
      },
    });

    render(<App snapshot={initial} commandFacade={commandFacade} />);

    await user.type(screen.getByLabelText("WAV source path"), "/Users/adrian/imports/retry.wav");
    await user.click(screen.getByRole("button", { name: "Import WAV" }));

    expect(screen.getByLabelText("WAV source path")).toHaveValue("/Users/adrian/imports/retry.wav");
    expect(screen.getByRole("alert")).toHaveTextContent("WAV source file has an unsupported WAV header.");
  });

  it("uses explicit command readiness instead of exact detail copy", async () => {
    const user = userEvent.setup();
    const calls: Array<{ method: string; args?: unknown }> = [];
    const initial = connectedSnapshot({
      commandSurface: {
        ready: true,
        detail: "Local desktop commands are connected.",
      },
      recording: {
        ...getMockDesktopSnapshot().recording,
        state: "Complete",
        recovery_action: "Previous local desktop WAV artifacts are saved.",
      },
    });
    const returned = connectedSnapshot();
    const commandFacade = fakeCommandFacade({
      startRecording: async (args) => {
        calls.push({ method: "startRecording", args });
        return returned;
      },
    });

    render(<App snapshot={initial} commandFacade={commandFacade} />);

    await user.click(screen.getByRole("button", { name: "Start recording" }));

    expect(calls).toEqual([{ method: "startRecording", args: undefined }]);
  });

  it("enables stop for active recordings and disables start until the returned snapshot lands", async () => {
    const user = userEvent.setup();
    const calls: Array<{ method: string; args?: unknown }> = [];
    const initial = connectedSnapshot();
    const returned = connectedSnapshot({
      recording: {
        ...initial.recording,
        state: "Complete",
        recovery_action: "Finalized local microphone and system audio WAV artifacts.",
      },
    });
    const commandFacade = fakeCommandFacade({
      stopRecording: async () => {
        calls.push({ method: "stopRecording" });
        return returned;
      },
    });

    render(<App snapshot={initial} commandFacade={commandFacade} />);

    expect(screen.getByRole("button", { name: "Start recording" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Stop recording" })).toBeEnabled();

    await user.click(screen.getByRole("button", { name: "Stop recording" }));

    expect(calls).toEqual([{ method: "stopRecording" }]);
    expect(screen.getByText("Finalized local microphone and system audio WAV artifacts.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Start recording" })).toBeEnabled();
  });

  it("transcribes the selected meeting through the desktop command and shows returned failures", async () => {
    const user = userEvent.setup();
    const calls: Array<{ method: string; args?: unknown }> = [];
    const initial = connectedSnapshot({
      recording: {
        ...getMockDesktopSnapshot().recording,
        state: "Complete",
        recovery_action: "Previous local desktop WAV artifacts are saved.",
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
    const commandFacade = fakeCommandFacade({
      transcribeMeeting: async (args) => {
        calls.push({ method: "transcribeMeeting", args });
        return returned;
      },
    });

    render(<App snapshot={initial} commandFacade={commandFacade} />);

    await user.click(screen.getByRole("button", { name: "Transcribe" }));

    expect(calls).toEqual([{ method: "transcribeMeeting", args: { meetingId: "circuit-review" } }]);
    expect(screen.getAllByText("Transcription failed").length).toBeGreaterThan(0);
    expect(screen.getByText("Whisper model is unavailable. Set CURIOSITY_WHISPER_MODEL.")).toBeInTheDocument();
  });

  it("blocks transcription until the saved Whisper path has matching Test path evidence", async () => {
    const user = userEvent.setup();
    const calls: Array<{ method: string; args?: unknown }> = [];
    const initial = connectedSnapshot({
      model: {
        kind: "untested",
        configuredPath: "/models/ggml-base.en.bin",
      },
    });
    const commandFacade = fakeCommandFacade({
      transcribeMeeting: async (args) => {
        calls.push({ method: "transcribeMeeting", args });
        return initial;
      },
    });

    render(<App snapshot={initial} commandFacade={commandFacade} />);

    const transcribe = screen.getByRole("button", { name: "Transcribe" });
    expect(transcribe).toBeDisabled();
    expect(transcribe).toHaveAttribute(
      "title",
      "Run Test path for the saved Whisper model file before transcription.",
    );
    expect(screen.getByText("Whisper path untested")).toBeInTheDocument();
    await user.click(transcribe);

    expect(calls).toEqual([]);
  });

  it("blocks transcription for unsupported Whisper files with choose-model guidance", async () => {
    const user = userEvent.setup();
    const calls: Array<{ method: string; args?: unknown }> = [];
    const initial = unsupportedWhisperSnapshot("/models/notes.txt");
    const commandFacade = fakeCommandFacade({
      transcribeMeeting: async (args) => {
        calls.push({ method: "transcribeMeeting", args });
        return initial;
      },
    });

    render(<App snapshot={initial} commandFacade={commandFacade} />);

    const transcribe = screen.getByRole("button", { name: "Transcribe" });
    expect(transcribe).toBeDisabled();
    expect(transcribe).toHaveAttribute(
      "title",
      "Choose a supported .bin or .gguf Whisper model file before transcription.",
    );
    expect(screen.getAllByText("Whisper file unsupported").length).toBeGreaterThan(0);
    expect(screen.queryByTitle("Run Test path for the saved Whisper model file before transcription.")).not.toBeInTheDocument();
    await user.click(transcribe);

    expect(calls).toEqual([]);
  });

  it("renames the selected meeting through the desktop command", async () => {
    const user = userEvent.setup();
    const calls: Array<{ method: string; args?: unknown }> = [];
    const initial = connectedSnapshot();
    const returned = connectedSnapshot({
      meetings: initial.meetings.map((meeting) =>
        meeting.id === "circuit-review" ? { ...meeting, title: "Renamed Planning" } : meeting,
      ),
      selectedMeetingId: "circuit-review",
    });
    const commandFacade = fakeCommandFacade({
      renameMeeting: async (args) => {
        calls.push({ method: "renameMeeting", args });
        return returned;
      },
    });

    render(<App snapshot={initial} commandFacade={commandFacade} />);

    await user.clear(screen.getByLabelText("Selected meeting title"));
    await user.type(screen.getByLabelText("Selected meeting title"), "Renamed Planning");
    await user.click(screen.getByRole("button", { name: "Rename" }));

    expect(calls).toEqual([
      {
        method: "renameMeeting",
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
    const calls: Array<{ method: string; args?: unknown }> = [];
    const initial = connectedSnapshot();
    const returned = connectedSnapshot({
      meetings: initial.meetings.map((meeting) =>
        meeting.id === "circuit-review"
          ? {
              ...meeting,
              exportState: {
                state: "exported",
                meetingId: "circuit-review",
                format: "json",
                path: "/tmp/circuit-review.json",
              },
            }
          : meeting,
      ),
      exportCommand: {
        state: "exported",
        meetingId: "circuit-review",
        format: "json",
        path: "/tmp/circuit-review.json",
      },
    });
    const commandFacade = fakeCommandFacade({
      exportMeeting: async (args) => {
        calls.push({ method: "exportMeeting", args });
        return returned;
      },
    });

    render(<App snapshot={initial} commandFacade={commandFacade} />);

    await user.click(screen.getByRole("button", { name: "Export JSON" }));

    expect(calls).toEqual([
      {
        method: "exportMeeting",
        args: { meetingId: "circuit-review", format: "json" },
      },
    ]);
    expect(screen.getAllByText("JSON exported").length).toBeGreaterThan(0);
    expect(screen.getAllByText("/tmp/circuit-review.json").length).toBeGreaterThan(0);
  });

  it("exports Markdown and SRT through the generic desktop command", async () => {
    const user = userEvent.setup();
    const calls: Array<{ method: string; args?: unknown }> = [];
    const initial = connectedSnapshot();
    const commandFacade = fakeCommandFacade({
      exportMeeting: async (args) => {
        calls.push({ method: "exportMeeting", args });
        return connectedSnapshot({
          meetings: initial.meetings.map((meeting) =>
            meeting.id === "circuit-review"
              ? {
                  ...meeting,
                  exportState: {
                    state: "exported",
                    meetingId: "circuit-review",
                    format: args.format,
                    path: `/tmp/circuit-review.${args.format === "markdown" ? "md" : "srt"}`,
                  },
                }
              : meeting,
          ),
          exportCommand: {
            state: "exported",
            meetingId: "circuit-review",
            format: args.format,
            path: `/tmp/circuit-review.${args.format === "markdown" ? "md" : "srt"}`,
          },
        });
      },
    });

    render(<App snapshot={initial} commandFacade={commandFacade} />);

    await user.selectOptions(screen.getByLabelText("Export format"), "markdown");
    await user.click(screen.getByRole("button", { name: "Export Markdown" }));
    await user.selectOptions(screen.getByLabelText("Export format"), "srt");
    await user.click(screen.getByRole("button", { name: "Export SRT" }));

    expect(calls).toEqual([
      {
        method: "exportMeeting",
        args: { meetingId: "circuit-review", format: "markdown" },
      },
      {
        method: "exportMeeting",
        args: { meetingId: "circuit-review", format: "srt" },
      },
    ]);
    expect(screen.getAllByText("SRT exported").length).toBeGreaterThan(0);
    expect(screen.getAllByText("/tmp/circuit-review.srt").length).toBeGreaterThan(0);
  });

  it("deletes private data through the desktop command while showing remaining exports", async () => {
    const user = userEvent.setup();
    const calls: Array<{ method: string; args?: unknown }> = [];
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
    const commandFacade = fakeCommandFacade({
      deleteMeeting: async (args) => {
        calls.push({ method: "deleteMeeting", args });
        return returned;
      },
    });

    render(<App snapshot={initial} commandFacade={commandFacade} />);

    await user.click(screen.getByRole("button", { name: "Delete private data" }));

    expect(calls).toEqual([
      {
        method: "deleteMeeting",
        args: { meetingId: "circuit-review" },
      },
    ]);
    expect(within(screen.getByLabelText("Meetings")).queryByText("Circuit Review")).not.toBeInTheDocument();
    expect(screen.getByText("Private artifacts deleted")).toBeInTheDocument();
    expect(screen.getByText("1 private artifact removed. 1 exported file remains outside app control.")).toBeInTheDocument();
  });

  it("disables private-data deletion only for the selected meeting with an active command job", () => {
    const selectedActive = connectedSnapshot({
      transcriptionJob: {
        id: "transcription-circuit-review-1700000001000",
        kind: "Transcription",
        meetingId: "circuit-review",
        state: "Running",
        cancelRequested: false,
        startedAtMs: 1_700_000_001_000,
      },
    });
    const otherMeetingActive = connectedSnapshot({
      summaryJob: {
        id: "summary-design-standup-1700000002000",
        kind: "Summary",
        meetingId: "design-standup",
        state: "Running",
        cancelRequested: false,
        startedAtMs: 1_700_000_002_000,
      },
    });

    const { rerender } = render(<App snapshot={selectedActive} commandFacade={fakeCommandFacade()} />);

    expect(screen.getByRole("button", { name: "Delete private data" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Delete private data" })).toHaveAttribute(
      "title",
      "Cancel or wait for the active transcription or summary job before deleting private data.",
    );

    rerender(<App snapshot={otherMeetingActive} commandFacade={fakeCommandFacade()} />);

    expect(screen.getByRole("button", { name: "Delete private data" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "Delete private data" })).toHaveAttribute(
      "title",
      "Delete app-private data for the selected meeting.",
    );
  });

  it("retries a failed delete through the preserved command meeting id", async () => {
    const user = userEvent.setup();
    const calls: Array<{ method: string; args?: unknown }> = [];
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
    const commandFacade = fakeCommandFacade({
      deleteMeeting: async (args) => {
        calls.push({ method: "deleteMeeting", args });
        return returned;
      },
    });

    render(<App snapshot={initial} commandFacade={commandFacade} />);

    expect(screen.getByRole("heading", { name: "Design Standup" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Retry delete" }));

    expect(calls).toEqual([
      {
        method: "deleteMeeting",
        args: { meetingId: "circuit-review" },
      },
    ]);
    expect(screen.getByText("Private artifacts deleted")).toBeInTheDocument();
  });

  it("tests and saves the configured Whisper path through desktop commands", async () => {
    const user = userEvent.setup();
    const calls: Array<{ method: string; args?: unknown }> = [];
    const initial = connectedSnapshot({
      settings: {
        whisperModelPath: "",
        ollamaBaseUrl: "http://127.0.0.1:11434",
        ollamaModel: "qwen3.6:27b",
        exportDirectory: null,
        rawAudioRetentionPolicy: "Retain",
      },
    });
    const saved = connectedSnapshot({
      model: {
        kind: "ready",
        configuredPath: "/models/ggml-base.en.bin",
      },
      setupGuidance: whisperSetupGuidanceForPath(
        "/models/ggml-base.en.bin",
        validWhisperPathTestEvidence("/models/ggml-base.en.bin"),
      ),
      settings: {
        whisperModelPath: "/models/ggml-base.en.bin",
        ollamaBaseUrl: "http://127.0.0.1:11434",
        ollamaModel: "qwen3.6:27b",
        exportDirectory: null,
        rawAudioRetentionPolicy: "Retain",
      },
    });
    const commandFacade = fakeCommandFacade({
      testWhisperModelPath: async (args) => {
        calls.push({ method: "testWhisperModelPath", args });
        return {
          state: "Valid",
          message: "Whisper model path is readable.",
          setupGuidance: "",
          fileSizeBytes: 16,
          sha256: "8b68af71d2eaaec61d5b4f50e330493cc0074323676962d9761cbc7c6810ba54",
        };
      },
      saveWhisperModelPath: async (args) => {
        calls.push({ method: "saveWhisperModelPath", args });
        return saved;
      },
    });

    render(<App snapshot={initial} commandFacade={commandFacade} />);

    await user.type(screen.getByLabelText("Whisper model path"), "/models/ggml-base.en.bin");
    await user.click(screen.getByRole("button", { name: "Test path" }));
    const feedback = await screen.findByRole("status");
    expect(
      within(feedback).getByText("Whisper model path is readable. Save Whisper to make this path active."),
    ).toBeInTheDocument();
    expect(within(feedback).getByText("Size: 16 bytes")).toBeInTheDocument();
    expect(
      within(feedback).getByText("SHA-256: 8b68af71d2eaaec61d5b4f50e330493cc0074323676962d9761cbc7c6810ba54"),
    ).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Save Whisper" }));

    expect(calls).toEqual([
      {
        method: "testWhisperModelPath",
        args: { path: "/models/ggml-base.en.bin" },
      },
      {
        method: "saveWhisperModelPath",
        args: { whisperModelPath: "/models/ggml-base.en.bin" },
      },
    ]);
    expect(screen.getByText("Whisper model path saved.")).toBeInTheDocument();
    expect(screen.getByLabelText("Whisper model path")).toHaveValue("/models/ggml-base.en.bin");
  });

  it("keeps a valid unsaved Whisper path inactive until it is saved", async () => {
    const user = userEvent.setup();
    const calls: Array<{ method: string; args?: unknown }> = [];
    const currentPath = "/models/current.gguf";
    const unsavedPath = "/models/unsaved.gguf";
    const initial = connectedSnapshot({
      model: {
        kind: "untested",
        configuredPath: currentPath,
      },
      setupGuidance: whisperSetupGuidanceForPath(currentPath, null),
      settings: {
        whisperModelPath: "",
        ollamaBaseUrl: "http://127.0.0.1:11434",
        ollamaModel: "qwen3.6:27b",
        exportDirectory: null,
        rawAudioRetentionPolicy: "Retain",
      },
    });
    const refreshedReady = connectedSnapshot({
      model: {
        kind: "ready",
        configuredPath: unsavedPath,
      },
      setupGuidance: whisperSetupGuidanceForPath(unsavedPath, validWhisperPathTestEvidence(unsavedPath)),
      settings: {
        ...initial.settings,
        whisperModelPath: unsavedPath,
      },
    });
    const commandFacade = fakeCommandFacade({
      desktopSnapshot: async () => {
        calls.push({ method: "desktopSnapshot" });
        return refreshedReady;
      },
      testWhisperModelPath: async (args) => {
        calls.push({ method: "testWhisperModelPath", args });
        return {
          state: "Valid",
          message: "Backend verified this Whisper model path.",
          setupGuidance: "",
          fileSizeBytes: 16,
          sha256: "8b68af71d2eaaec61d5b4f50e330493cc0074323676962d9761cbc7c6810ba54",
        };
      },
    });

    render(<App snapshot={initial} commandFacade={commandFacade} />);

    const transcribe = screen.getByRole("button", { name: "Transcribe" });
    expect(transcribe).toBeDisabled();
    await user.type(screen.getByLabelText("Whisper model path"), unsavedPath);
    await user.click(screen.getByRole("button", { name: "Test path" }));

    const feedback = await screen.findByRole("status");
    expect(
      within(feedback).getByText("Backend verified this Whisper model path. Save Whisper to make this path active."),
    ).toBeInTheDocument();
    expect(within(feedback).getByText("Size: 16 bytes")).toBeInTheDocument();
    expect(transcribe).toBeDisabled();
    expect(calls).toEqual([
      {
        method: "testWhisperModelPath",
        args: { path: unsavedPath },
      },
    ]);
  });

  it("does not add save guidance to invalid Whisper path test feedback", async () => {
    const user = userEvent.setup();
    const calls: Array<{ method: string; args?: unknown }> = [];
    const initial = connectedSnapshot({
      model: {
        kind: "untested",
        configuredPath: "/models/current.gguf",
      },
      setupGuidance: whisperSetupGuidanceForPath("/models/current.gguf", null),
      settings: {
        whisperModelPath: "",
        ollamaBaseUrl: "http://127.0.0.1:11434",
        ollamaModel: "qwen3.6:27b",
        exportDirectory: null,
        rawAudioRetentionPolicy: "Retain",
      },
    });
    const commandFacade = fakeCommandFacade({
      desktopSnapshot: async () => {
        calls.push({ method: "desktopSnapshot" });
        return connectedSnapshot();
      },
      testWhisperModelPath: async (args) => {
        calls.push({ method: "testWhisperModelPath", args });
        return {
          state: "Invalid",
          message: "Whisper model path is blocked.",
          setupGuidance: "Choose an existing whisper.cpp-compatible .bin or .gguf model file.",
        };
      },
    });

    render(<App snapshot={initial} commandFacade={commandFacade} />);

    await user.type(screen.getByLabelText("Whisper model path"), "/models/blocked.txt");
    await user.click(screen.getByRole("button", { name: "Test path" }));

    const feedback = await screen.findByRole("status");
    expect(within(feedback).getByText("Whisper model path is blocked.")).toBeInTheDocument();
    expect(within(feedback).queryByText(/Save Whisper to make this path active/)).not.toBeInTheDocument();
    expect(calls).toEqual([
      {
        method: "testWhisperModelPath",
        args: { path: "/models/blocked.txt" },
      },
    ]);
  });

  it("enables transcription after testing the already-saved Whisper path", async () => {
    const user = userEvent.setup();
    const calls: Array<{ method: string; args?: unknown }> = [];
    const savedUntested = connectedSnapshot({
      model: {
        kind: "untested",
        configuredPath: "/models/ggml-base.en.bin",
      },
      setupGuidance: whisperSetupGuidanceForPath("/models/ggml-base.en.bin", null),
      settings: {
        whisperModelPath: "/models/ggml-base.en.bin",
        ollamaBaseUrl: "http://127.0.0.1:11434",
        ollamaModel: "qwen3.6:27b",
        exportDirectory: null,
        rawAudioRetentionPolicy: "Retain",
      },
    });
    const refreshedReady = connectedSnapshot({
      model: {
        kind: "ready",
        configuredPath: "/models/ggml-base.en.bin",
      },
      setupGuidance: whisperSetupGuidanceForPath(
        "/models/ggml-base.en.bin",
        validWhisperPathTestEvidence("/models/ggml-base.en.bin"),
      ),
      settings: savedUntested.settings,
    });
    const commandFacade = fakeCommandFacade({
      desktopSnapshot: async () => {
        calls.push({ method: "desktopSnapshot" });
        return refreshedReady;
      },
      testWhisperModelPath: async (args) => {
        calls.push({ method: "testWhisperModelPath", args });
        return {
          state: "Valid",
          message: "Whisper model path is readable.",
          setupGuidance: "",
          fileSizeBytes: 16,
          sha256: "8b68af71d2eaaec61d5b4f50e330493cc0074323676962d9761cbc7c6810ba54",
        };
      },
    });

    render(<App snapshot={savedUntested} commandFacade={commandFacade} />);

    const transcribe = screen.getByRole("button", { name: "Transcribe" });
    expect(transcribe).toBeDisabled();
    await user.click(screen.getByRole("button", { name: "Test path" }));

    await waitFor(() => expect(transcribe).toBeEnabled());
    expect(calls).toEqual([
      {
        method: "testWhisperModelPath",
        args: { path: "/models/ggml-base.en.bin" },
      },
      { method: "desktopSnapshot" },
    ]);
  });

  it("enables transcription after testing the environment fallback Whisper path", async () => {
    const user = userEvent.setup();
    const calls: Array<{ method: string; args?: unknown }> = [];
    const envPath = "/env/models/ggml-base.en.bin";
    const envUntested = connectedSnapshot({
      model: {
        kind: "untested",
        configuredPath: envPath,
      },
      setupGuidance: whisperSetupGuidanceForPath(envPath, null),
      settings: {
        whisperModelPath: "",
        ollamaBaseUrl: "http://127.0.0.1:11434",
        ollamaModel: "qwen3.6:27b",
        exportDirectory: null,
        rawAudioRetentionPolicy: "Retain",
      },
    });
    const refreshedReady = connectedSnapshot({
      model: {
        kind: "ready",
        configuredPath: envPath,
      },
      setupGuidance: whisperSetupGuidanceForPath(envPath, validWhisperPathTestEvidence(envPath)),
      settings: envUntested.settings,
    });
    const commandFacade = fakeCommandFacade({
      desktopSnapshot: async () => {
        calls.push({ method: "desktopSnapshot" });
        return refreshedReady;
      },
      testWhisperModelPath: async (args) => {
        calls.push({ method: "testWhisperModelPath", args });
        return {
          state: "Valid",
          message: "Whisper model path is readable.",
          setupGuidance: "",
          fileSizeBytes: 16,
          sha256: "8b68af71d2eaaec61d5b4f50e330493cc0074323676962d9761cbc7c6810ba54",
        };
      },
    });

    render(<App snapshot={envUntested} commandFacade={commandFacade} />);

    const transcribe = screen.getByRole("button", { name: "Transcribe" });
    expect(transcribe).toBeDisabled();
    await user.type(screen.getByLabelText("Whisper model path"), envPath);
    await user.click(screen.getByRole("button", { name: "Test path" }));

    await waitFor(() => expect(transcribe).toBeEnabled());
    expect(screen.getByLabelText("Whisper model path")).toHaveValue(envPath);
    expect(calls).toEqual([
      {
        method: "testWhisperModelPath",
        args: { path: envPath },
      },
      { method: "desktopSnapshot" },
    ]);
  });

  it("preserves a typed Whisper model path when model picking is canceled", async () => {
    const user = userEvent.setup();
    const initial = connectedSnapshot({
      settings: {
        whisperModelPath: "",
        ollamaBaseUrl: "http://127.0.0.1:11434",
        ollamaModel: "qwen3.6:27b",
        exportDirectory: null,
        rawAudioRetentionPolicy: "Retain",
      },
    });

    render(
      <App
        snapshot={initial}
        commandFacade={fakeCommandFacade()}
        filePicker={{
          chooseImportWavPath: async () => null,
          chooseWhisperModelPath: async () => null,
        }}
      />,
    );

    await user.type(screen.getByLabelText("Whisper model path"), "/Users/adrian/models/typed.bin");
    await user.click(screen.getByRole("button", { name: "Choose model" }));

    expect(screen.getByLabelText("Whisper model path")).toHaveValue("/Users/adrian/models/typed.bin");
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("preserves a typed Whisper model path and reports model picker errors in settings feedback", async () => {
    const user = userEvent.setup();
    const initial = connectedSnapshot({
      settings: {
        whisperModelPath: "",
        ollamaBaseUrl: "http://127.0.0.1:11434",
        ollamaModel: "qwen3.6:27b",
        exportDirectory: null,
        rawAudioRetentionPolicy: "Retain",
      },
    });

    render(
      <App
        snapshot={initial}
        commandFacade={fakeCommandFacade()}
        filePicker={{
          chooseImportWavPath: async () => null,
          chooseWhisperModelPath: async () => {
            throw new Error("native model dialog failed");
          },
        }}
      />,
    );

    await user.type(screen.getByLabelText("Whisper model path"), "/Users/adrian/models/typed.bin");
    await user.click(screen.getByRole("button", { name: "Choose model" }));

    expect(screen.getByLabelText("Whisper model path")).toHaveValue("/Users/adrian/models/typed.bin");
    expect(screen.getByRole("status")).toHaveTextContent("native model dialog failed");
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("uses the picked Whisper model path for existing test and save actions", async () => {
    const user = userEvent.setup();
    const calls: Array<{ method: string; args?: unknown }> = [];
    const initial = connectedSnapshot({
      settings: {
        whisperModelPath: "",
        ollamaBaseUrl: "http://127.0.0.1:11434",
        ollamaModel: "qwen3.6:27b",
        exportDirectory: null,
        rawAudioRetentionPolicy: "Retain",
      },
    });
    const saved = connectedSnapshot({
      model: {
        kind: "ready",
        configuredPath: "/Users/adrian/models/chosen.gguf",
      },
      setupGuidance: whisperSetupGuidanceForPath(
        "/Users/adrian/models/chosen.gguf",
        validWhisperPathTestEvidence("/Users/adrian/models/chosen.gguf"),
      ),
      settings: {
        whisperModelPath: "/Users/adrian/models/chosen.gguf",
        ollamaBaseUrl: "http://127.0.0.1:11434",
        ollamaModel: "qwen3.6:27b",
        exportDirectory: null,
        rawAudioRetentionPolicy: "Retain",
      },
    });
    const commandFacade = fakeCommandFacade({
      testWhisperModelPath: async (args) => {
        calls.push({ method: "testWhisperModelPath", args });
        return {
          state: "Valid",
          message: "Whisper model path is readable.",
          setupGuidance: "",
          fileSizeBytes: 16,
          sha256: "8b68af71d2eaaec61d5b4f50e330493cc0074323676962d9761cbc7c6810ba54",
        };
      },
      saveWhisperModelPath: async (args) => {
        calls.push({ method: "saveWhisperModelPath", args });
        return saved;
      },
    });

    render(
      <App
        snapshot={initial}
        commandFacade={commandFacade}
        filePicker={{
          chooseImportWavPath: async () => null,
          chooseWhisperModelPath: async () => "/Users/adrian/models/chosen.gguf",
        }}
      />,
    );

    await user.click(screen.getByRole("button", { name: "Choose model" }));
    await user.click(screen.getByRole("button", { name: "Test path" }));
    await screen.findByText("Whisper model path is readable. Save Whisper to make this path active.");
    await user.click(screen.getByRole("button", { name: "Save Whisper" }));

    expect(calls).toEqual([
      {
        method: "testWhisperModelPath",
        args: { path: "/Users/adrian/models/chosen.gguf" },
      },
      {
        method: "saveWhisperModelPath",
        args: { whisperModelPath: "/Users/adrian/models/chosen.gguf" },
      },
    ]);
    expect(screen.getByLabelText("Whisper model path")).toHaveValue("/Users/adrian/models/chosen.gguf");
  });

  it("disables native Whisper model picking when commands are unavailable or another command is busy", async () => {
    const user = userEvent.setup();
    const unavailable = connectedSnapshot({
      commandSurface: {
        ready: false,
        detail: "Desktop command surface is unavailable.",
      },
    });
    const initial = connectedSnapshot({
      recording: {
        ...getMockDesktopSnapshot().recording,
        state: "Complete",
        recovery_action: "Previous local desktop WAV artifacts are saved.",
      },
    });
    const returnedAfterStart = connectedSnapshot({
      recording: {
        ...getMockDesktopSnapshot().recording,
        state: "Recording",
        permission_state: "Ready",
        recovery_action: "Recording desktop audio.",
      },
    });
    let finishCommand: () => void = () => undefined;
    const pendingSnapshot = new Promise<ReturnType<typeof connectedSnapshot>>((resolve) => {
      finishCommand = () => resolve(returnedAfterStart);
    });
    const commandFacade = fakeCommandFacade({
      startRecording: async () => pendingSnapshot,
    });

    const { rerender } = render(
      <App
        snapshot={unavailable}
        commandFacade={fakeCommandFacade()}
        filePicker={{
          chooseImportWavPath: async () => null,
          chooseWhisperModelPath: async () => "/Users/adrian/models/chosen.gguf",
        }}
      />,
    );

    expect(screen.getByRole("button", { name: "Choose model" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Test path" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "Save Whisper" })).toBeEnabled();

    rerender(
      <App
        snapshot={initial}
        commandFacade={commandFacade}
        filePicker={{
          chooseImportWavPath: async () => null,
          chooseWhisperModelPath: async () => "/Users/adrian/models/chosen.gguf",
        }}
      />,
    );
    await user.click(screen.getByRole("button", { name: "Start recording" }));
    expect(screen.getByRole("button", { name: "Choose model" })).toBeDisabled();

    finishCommand();
    expect(await screen.findByRole("button", { name: "Choose model" })).toBeEnabled();
  });

  it("saves local analysis settings through the desktop command", async () => {
    const user = userEvent.setup();
    const calls: Array<{ method: string; args?: unknown }> = [];
    const initial = connectedSnapshot({
      settings: {
        whisperModelPath: "",
        ollamaBaseUrl: "http://127.0.0.1:11434",
        ollamaModel: "qwen3.6:27b",
        exportDirectory: null,
        rawAudioRetentionPolicy: "Retain",
      },
    });
    const returned = connectedSnapshot({
      settings: {
        whisperModelPath: "",
        ollamaBaseUrl: "http://127.0.0.1:11435",
        ollamaModel: "gemma4:31b",
        exportDirectory: null,
        rawAudioRetentionPolicy: "Retain",
      },
    });
    const commandFacade = fakeCommandFacade({
      saveAnalysisSettings: async (args) => {
        calls.push({ method: "saveAnalysisSettings", args });
        return returned;
      },
    });

    render(<App snapshot={initial} commandFacade={commandFacade} />);

    await user.clear(screen.getByLabelText("Ollama base URL"));
    await user.type(screen.getByLabelText("Ollama base URL"), "http://127.0.0.1:11435");
    await user.clear(screen.getByLabelText("Ollama model"));
    await user.type(screen.getByLabelText("Ollama model"), "gemma4:31b");
    await user.click(screen.getByRole("button", { name: "Save analysis" }));

    expect(calls).toEqual([
      {
        method: "saveAnalysisSettings",
        args: {
          ollamaBaseUrl: "http://127.0.0.1:11435",
          ollamaModel: "gemma4:31b",
        },
      },
    ]);
    expect(screen.getByLabelText("Ollama model")).toHaveValue("gemma4:31b");
  });

  it("saves the default raw-audio retention policy through the desktop command", async () => {
    const user = userEvent.setup();
    const calls: Array<{ method: string; args?: unknown }> = [];
    const initial = connectedSnapshot({
      settings: {
        whisperModelPath: "",
        ollamaBaseUrl: "http://127.0.0.1:11434",
        ollamaModel: "qwen3.6:27b",
        exportDirectory: null,
        rawAudioRetentionPolicy: "Retain",
      },
    });
    const returned = connectedSnapshot({
      settings: {
        whisperModelPath: "",
        ollamaBaseUrl: "http://127.0.0.1:11434",
        ollamaModel: "qwen3.6:27b",
        exportDirectory: null,
        rawAudioRetentionPolicy: "DeleteAfterTranscription",
      },
    });
    const commandFacade = fakeCommandFacade({
      saveRawAudioRetentionPolicy: async (args) => {
        calls.push({ method: "saveRawAudioRetentionPolicy", args });
        return returned;
      },
    });

    render(<App snapshot={initial} commandFacade={commandFacade} />);

    expect(screen.queryByRole("option", { name: "Never save" })).not.toBeInTheDocument();
    await user.selectOptions(screen.getByLabelText("Raw audio retention"), "DeleteAfterTranscription");
    await user.click(screen.getByRole("button", { name: "Save retention" }));

    expect(calls).toEqual([
      {
        method: "saveRawAudioRetentionPolicy",
        args: { rawAudioRetentionPolicy: "DeleteAfterTranscription" },
      },
    ]);
    expect(screen.getByLabelText("Raw audio retention")).toHaveValue("DeleteAfterTranscription");
    expect(screen.getByText("Raw-audio retention saved.")).toBeInTheDocument();
  });

  it("tests configured Ollama reachability from settings", async () => {
    const user = userEvent.setup();
    const calls: Array<{ method: string; args?: unknown }> = [];
    const settings = {
      whisperModelPath: "",
      ollamaBaseUrl: "http://127.0.0.1:11434",
      ollamaModel: "qwen3.6:27b",
      exportDirectory: null,
      rawAudioRetentionPolicy: "Retain" as const,
    };
    const initial = connectedSnapshot({
      settings,
      setupGuidance: ollamaSetupGuidance({
        baseUrl: settings.ollamaBaseUrl,
        model: settings.ollamaModel,
      }),
    });
    const refreshed = connectedSnapshot({
      settings,
      setupGuidance: ollamaSetupGuidance({
        baseUrl: settings.ollamaBaseUrl,
        model: settings.ollamaModel,
        availability: "AvailableAtLastTest",
        message: "Ollama was available at the last manual test.",
        setupGuidance: "Summaries can run with local Ollama based on the last explicit test.",
        lastConnectionTest: {
          baseUrl: settings.ollamaBaseUrl,
          requestedModel: settings.ollamaModel,
          testedAtMs: 1_700_000_003_000,
          state: "Available",
          selectedLocalModelTag: "qwen3.6:27b",
          installedLocalModels: ["gemma4:31b", "qwen3.6:27b"],
          pullCommand: null,
          failureDetail: null,
        },
      }),
    });
    const commandFacade = fakeCommandFacade({
      testOllamaConnection: async (args) => {
        calls.push({ method: "testOllamaConnection", args });
        return {
          state: "Available",
          message: "Ollama is reachable and qwen3.6:27b is installed.",
          setupGuidance: "",
          selectedLocalModelTag: "qwen3.6:27b",
          installedLocalModels: ["gemma4:31b", "qwen3.6:27b"],
          pullCommand: null,
        };
      },
      desktopSnapshot: async () => {
        calls.push({ method: "desktopSnapshot" });
        return refreshed;
      },
    });

    render(<App snapshot={initial} commandFacade={commandFacade} />);

    await user.click(screen.getByRole("button", { name: "Test Ollama" }));

    expect(calls).toEqual([
      {
        method: "testOllamaConnection",
        args: {
          baseUrl: "http://127.0.0.1:11434",
          model: "qwen3.6:27b",
        },
      },
      { method: "desktopSnapshot" },
    ]);
    const readiness = screen.getByLabelText("Model readiness guidance");
    expect(within(readiness).getByText("Ollama available at last test")).toBeInTheDocument();
    expect(within(readiness).getByText("Ollama was available at the last manual test.")).toBeInTheDocument();
    expect(readiness).toHaveTextContent("Last explicit Test Ollama: Available at 2023-11-14T22:13:23.000Z");
    expect(screen.getByText("Ollama is reachable and qwen3.6:27b is installed.")).toBeInTheDocument();
    expect(screen.getByText("Installed models: gemma4:31b, qwen3.6:27b")).toBeInTheDocument();
  });

  it("refreshes readiness with the manual Ollama pull command when the selected saved model is missing", async () => {
    const user = userEvent.setup();
    const calls: Array<{ method: string; args?: unknown }> = [];
    const copiedCommands: string[] = [];
    const settings = {
      whisperModelPath: "",
      ollamaBaseUrl: "http://127.0.0.1:11434",
      ollamaModel: "qwen3.6:27b",
      exportDirectory: null,
      rawAudioRetentionPolicy: "Retain" as const,
    };
    const initial = connectedSnapshot({
      settings,
      setupGuidance: ollamaSetupGuidance({
        baseUrl: settings.ollamaBaseUrl,
        model: settings.ollamaModel,
      }),
    });
    const refreshed = connectedSnapshot({
      settings,
      setupGuidance: ollamaSetupGuidance({
        baseUrl: settings.ollamaBaseUrl,
        model: settings.ollamaModel,
        availability: "MissingModelAtLastTest",
        message: "Ollama is reachable, but qwen3.6:27b is not installed.",
        setupGuidance: "Install the selected model with `ollama pull qwen3.6:27b`, then retry.",
        lastConnectionTest: {
          baseUrl: settings.ollamaBaseUrl,
          requestedModel: settings.ollamaModel,
          testedAtMs: 1_700_000_003_000,
          state: "Unavailable",
          selectedLocalModelTag: "qwen3.6:27b",
          installedLocalModels: ["gemma4:31b"],
          pullCommand: "ollama pull qwen3.6:27b",
          failureDetail: "Ollama is reachable, but qwen3.6:27b is not installed.",
        },
      }),
    });
    const commandFacade = fakeCommandFacade({
      testOllamaConnection: async (args) => {
        calls.push({ method: "testOllamaConnection", args });
        return {
          state: "Unavailable",
          message: "Ollama is reachable, but qwen3.6:27b is not installed.",
          setupGuidance: "Install the selected model with `ollama pull qwen3.6:27b`, then retry.",
          selectedLocalModelTag: "qwen3.6:27b",
          installedLocalModels: ["gemma4:31b"],
          pullCommand: "ollama pull qwen3.6:27b",
        };
      },
      desktopSnapshot: async () => {
        calls.push({ method: "desktopSnapshot" });
        return refreshed;
      },
    });

    render(
      <App
        snapshot={initial}
        commandFacade={commandFacade}
        clipboardWriter={{
          writeText: async (text) => {
            copiedCommands.push(text);
          },
        }}
      />,
    );

    await user.click(screen.getByRole("button", { name: "Test Ollama" }));

    expect(calls).toEqual([
      {
        method: "testOllamaConnection",
        args: {
          baseUrl: "http://127.0.0.1:11434",
          model: "qwen3.6:27b",
        },
      },
      { method: "desktopSnapshot" },
    ]);
    const feedback = screen.getByRole("status");
    expect(within(feedback).getByText("Ollama is reachable, but qwen3.6:27b is not installed.")).toBeInTheDocument();
    expect(within(feedback).getByText("Installed models: gemma4:31b")).toBeInTheDocument();
    expect(within(feedback).getByText("Pull command: ollama pull qwen3.6:27b")).toBeInTheDocument();
    calls.length = 0;
    await user.click(within(feedback).getByRole("button", { name: "Copy pull command for qwen3.6:27b" }));

    expect(copiedCommands).toEqual(["ollama pull qwen3.6:27b"]);
    expect(screen.getByRole("status")).toHaveTextContent("Pull command copied.");
    expect(screen.getByRole("status")).toHaveTextContent("Pull command: ollama pull qwen3.6:27b");
    expect(calls).toEqual([]);

    const readiness = screen.getByLabelText("Model readiness guidance");
    expect(within(readiness).getByText("Ollama model missing")).toBeInTheDocument();
    expect(within(readiness).getByText("Pull command: ollama pull qwen3.6:27b")).toBeInTheDocument();
  });

  it("keeps Ollama readiness unchanged when testing unsaved analysis settings", async () => {
    const user = userEvent.setup();
    const calls: Array<{ method: string; args?: unknown }> = [];
    const initial = connectedSnapshot({
      settings: {
        whisperModelPath: "",
        ollamaBaseUrl: "http://127.0.0.1:11434",
        ollamaModel: "qwen3.6:27b",
        exportDirectory: null,
        rawAudioRetentionPolicy: "Retain",
      },
      setupGuidance: ollamaSetupGuidance({
        baseUrl: "http://127.0.0.1:11434",
        model: "qwen3.6:27b",
      }),
    });
    const commandFacade = fakeCommandFacade({
      testOllamaConnection: async (args) => {
        calls.push({ method: "testOllamaConnection", args });
        return {
          state: "Available",
          message: "Ollama is reachable and gemma4:31b is installed.",
          setupGuidance: "",
          selectedLocalModelTag: "gemma4:31b",
          installedLocalModels: ["gemma4:31b"],
          pullCommand: null,
        };
      },
      desktopSnapshot: async () => {
        calls.push({ method: "desktopSnapshot" });
        return connectedSnapshot();
      },
    });

    render(<App snapshot={initial} commandFacade={commandFacade} />);

    await user.clear(screen.getByLabelText("Ollama model"));
    await user.type(screen.getByLabelText("Ollama model"), "gemma4:31b");
    await user.click(screen.getByRole("button", { name: "Test Ollama" }));

    expect(calls).toEqual([
      {
        method: "testOllamaConnection",
        args: {
          baseUrl: "http://127.0.0.1:11434",
          model: "gemma4:31b",
        },
      },
    ]);
    const feedback = screen.getByRole("status");
    expect(within(feedback).getByText("Ollama is reachable and gemma4:31b is installed.")).toBeInTheDocument();
    const readiness = screen.getByLabelText("Model readiness guidance");
    expect(within(readiness).getByText("Ollama availability unknown")).toBeInTheDocument();
    expect(within(readiness).queryByText("Ollama available at last test")).not.toBeInTheDocument();
  });

  it("clears successful Ollama reachability feedback when tested inputs change", async () => {
    const user = userEvent.setup();
    const initial = connectedSnapshot({
      settings: {
        whisperModelPath: "",
        ollamaBaseUrl: "http://127.0.0.1:11434",
        ollamaModel: "qwen3.6:27b",
        exportDirectory: null,
        rawAudioRetentionPolicy: "Retain",
      },
    });
    const commandFacade = fakeCommandFacade({
      testOllamaConnection: async () => ({
        state: "Available",
        message: "Ollama is reachable and qwen3.6:27b is installed.",
        setupGuidance: "",
        selectedLocalModelTag: "qwen3.6:27b",
        installedLocalModels: ["qwen3.6:27b"],
        pullCommand: null,
      }),
    });

    render(<App snapshot={initial} commandFacade={commandFacade} />);

    await user.click(screen.getByRole("button", { name: "Test Ollama" }));
    await user.clear(screen.getByLabelText("Ollama model"));
    await user.type(screen.getByLabelText("Ollama model"), "gemma4:31b");

    expect(screen.queryByText("Ollama is reachable and qwen3.6:27b is installed.")).not.toBeInTheDocument();
  });

  it("generates a local summary for a selected meeting with transcript segments", async () => {
    const user = userEvent.setup();
    const calls: Array<{ method: string; args?: unknown }> = [];
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
    const commandFacade = fakeCommandFacade({
      generateSummary: async (args) => {
        calls.push({ method: "generateSummary", args });
        return returned;
      },
    });

    render(<App snapshot={initial} commandFacade={commandFacade} />);

    await user.click(screen.getByRole("button", { name: "Generate summary" }));

    expect(calls).toEqual([
      {
        method: "generateSummary",
        args: { meetingId: "circuit-review" },
      },
    ]);
    expect(screen.getByText("Local Ollama summary")).toBeInTheDocument();
    expect(screen.getAllByText(/ollama \/ qwen3.6:27b/i).length).toBeGreaterThan(0);
  });

  it("keeps summary generation disabled when last Ollama test found a missing model", async () => {
    const user = userEvent.setup();
    const calls: Array<{ method: string; args?: unknown }> = [];
    const setupGuidance =
      "Run `ollama pull qwen3.6:27b`, then run Test Ollama again. Availability is not checked in the background.";
    const initial = connectedSnapshot({
      setupGuidance: ollamaSetupGuidance({
        availability: "MissingModelAtLastTest",
        message:
          "Last explicit Test Ollama reached Ollama, but qwen3.6:27b was missing. Summaries are unavailable until the selected local model is installed.",
        setupGuidance,
        lastConnectionTest: {
          baseUrl: "http://127.0.0.1:11434",
          requestedModel: "qwen3.6:27b",
          testedAtMs: 1_700_000_003_000,
          state: "Unavailable",
          selectedLocalModelTag: "qwen3.6:27b",
          installedLocalModels: ["gemma4:31b"],
          pullCommand: "ollama pull qwen3.6:27b",
          failureDetail: "Ollama is reachable, but qwen3.6:27b is not installed.",
        },
      }),
    });
    const commandFacade = fakeCommandFacade({
      generateSummary: async (args) => {
        calls.push({ method: "generateSummary", args });
        return initial;
      },
    });

    render(<App snapshot={initial} commandFacade={commandFacade} />);

    const generate = screen.getByRole("button", { name: "Generate summary" });
    expect(generate).toBeDisabled();
    expect(generate).toHaveAttribute("title", setupGuidance);
    await user.click(generate);

    expect(calls).toEqual([]);
  });

  it("saves a user correction for the selected transcript segment", async () => {
    const user = userEvent.setup();
    vi.spyOn(Date, "now").mockReturnValue(1_700_000_003_000);
    const calls: Array<{ method: string; args?: unknown }> = [];
    const initial = connectedSnapshot();
    const correctedText = "We decided to keep raw audio retention visible and searchable.";
    const returned = connectedSnapshot({
      meetings: initial.meetings.map((meeting) =>
        meeting.id === "circuit-review"
          ? {
              ...meeting,
              transcriptText: `${correctedText} Exports should show when files remain outside app control.`,
              segments: meeting.segments.map((segment) =>
                segment.id === "segment-1"
                  ? {
                      ...segment,
                      text: correctedText,
                      originalText: "We decided to keep raw audio retention visible.",
                    }
                  : segment,
              ),
            }
          : meeting,
      ),
    });
    const commandFacade = fakeCommandFacade({
      correctTranscriptSegment: async (args) => {
        calls.push({ method: "correctTranscriptSegment", args });
        return returned;
      },
    });

    render(<App snapshot={initial} commandFacade={commandFacade} />);

    const firstSegment = screen
      .getByText("We decided to keep raw audio retention visible.")
      .closest("article");
    expect(firstSegment).not.toBeNull();
    await user.click(within(firstSegment!).getByRole("button", { name: "Edit segment" }));
    const editor = screen.getByLabelText("Transcript segment text");
    await user.clear(editor);
    await user.type(editor, correctedText);
    await user.click(screen.getByRole("button", { name: "Save correction" }));

    expect(calls).toEqual([
      {
        method: "correctTranscriptSegment",
        args: {
          meetingId: "circuit-review",
          segmentId: "segment-1",
          correctedText,
          editedAtMs: 1_700_000_003_000,
        },
      },
    ]);
    expect(screen.getByText(correctedText)).toBeInTheDocument();
    expect(screen.getByText("Original: We decided to keep raw audio retention visible.")).toBeInTheDocument();
  });

  it("keeps transcript editing scoped to one segment and cancels without saving", async () => {
    const user = userEvent.setup();
    const calls: unknown[] = [];
    const commandFacade = fakeCommandFacade({
      correctTranscriptSegment: async (args) => {
        calls.push(args);
        return connectedSnapshot();
      },
    });

    render(<App snapshot={connectedSnapshot()} commandFacade={commandFacade} />);

    const firstSegment = screen
      .getByText("We decided to keep raw audio retention visible.")
      .closest("article");
    const secondSegment = screen
      .getByText("Exports should show when files remain outside app control.")
      .closest("article");
    expect(firstSegment).not.toBeNull();
    expect(secondSegment).not.toBeNull();

    await user.click(within(firstSegment!).getByRole("button", { name: "Edit segment" }));
    await user.click(within(secondSegment!).getByRole("button", { name: "Edit segment" }));

    expect(screen.getAllByLabelText("Transcript segment text")).toHaveLength(1);
    expect(screen.getByLabelText("Transcript segment text")).toHaveValue(
      "Exports should show when files remain outside app control.",
    );

    await user.clear(screen.getByLabelText("Transcript segment text"));
    await user.type(screen.getByLabelText("Transcript segment text"), "Discarded correction");
    await user.click(screen.getByRole("button", { name: "Cancel correction" }));

    expect(calls).toEqual([]);
    expect(screen.queryByLabelText("Transcript segment text")).not.toBeInTheDocument();
    expect(screen.getByText("Exports should show when files remain outside app control.")).toBeInTheDocument();
  });

  it("renders visible job ownership state from the desktop snapshot", () => {
    const snapshot = connectedSnapshot({
      transcriptionJob: {
        id: "transcription-circuit-review-1700000001000",
        kind: "Transcription",
        meetingId: "circuit-review",
        state: "CancelRequested",
        cancelRequested: true,
        startedAtMs: 1_700_000_001_000,
      },
      summaryJob: {
        id: "summary-circuit-review-1700000002000",
        kind: "Summary",
        meetingId: "circuit-review",
        state: "Running",
        cancelRequested: false,
        startedAtMs: 1_700_000_002_000,
      },
    });

    render(<App snapshot={snapshot} commandFacade={fakeCommandFacade()} />);

    expect(screen.getByText("Transcription cancel requested")).toBeInTheDocument();
    expect(screen.getByText("Summary running")).toBeInTheDocument();
  });

  it("cancels visible transcription and summary jobs through the typed command facade", async () => {
    const user = userEvent.setup();
    const calls: Array<{ method: string; args?: unknown }> = [];
    let finishTranscriptionCancellation!: () => void;
    const snapshot = connectedSnapshot({
      transcriptionJob: {
        id: "transcription-circuit-review-1700000001000",
        kind: "Transcription",
        meetingId: "circuit-review",
        state: "Running",
        cancelRequested: false,
        startedAtMs: 1_700_000_001_000,
      },
      summaryJob: {
        id: "summary-circuit-review-1700000002000",
        kind: "Summary",
        meetingId: "circuit-review",
        state: "Running",
        cancelRequested: false,
        startedAtMs: 1_700_000_002_000,
      },
    });
    const pendingTranscriptionCancellation = new Promise<typeof snapshot>((resolve) => {
      finishTranscriptionCancellation = () => resolve(snapshot);
    });
    const commandFacade = fakeCommandFacade({
      cancelTranscription: async (args) => {
        calls.push({ method: "cancelTranscription", args });
        return pendingTranscriptionCancellation;
      },
      cancelSummary: async (args) => {
        calls.push({ method: "cancelSummary", args });
        return snapshot;
      },
    });

    render(<App snapshot={snapshot} commandFacade={commandFacade} />);

    await user.click(screen.getByRole("button", { name: "Cancel transcription" }));
    expect(screen.getByRole("button", { name: "Canceling transcription" })).toBeDisabled();
    await act(async () => {
      finishTranscriptionCancellation();
      await pendingTranscriptionCancellation;
    });
    const cancelSummary = screen.getByRole("button", { name: "Cancel summary" });
    await waitFor(() => expect(cancelSummary).toBeEnabled());
    await user.click(cancelSummary);

    expect(calls).toEqual([
      {
        method: "cancelTranscription",
        args: { jobId: "transcription-circuit-review-1700000001000" },
      },
      {
        method: "cancelSummary",
        args: { jobId: "summary-circuit-review-1700000002000" },
      },
    ]);
  });

  it("retries the selected meeting's recovered transcription job without a cancel control", async () => {
    const user = userEvent.setup();
    const calls: Array<{ method: string; args?: unknown }> = [];
    const snapshot = connectedSnapshot({
      transcriptionJob: {
        id: "transcription-circuit-review-1700000001000",
        kind: "Transcription",
        meetingId: "circuit-review",
        state: "Recovery",
        cancelRequested: false,
        startedAtMs: 1_700_000_001_000,
        lastError: "transcription worker was not running after app restart",
      },
    });
    const commandFacade = fakeCommandFacade({
      transcribeMeeting: async (args) => {
        calls.push({ method: "transcribeMeeting", args });
        return snapshot;
      },
    });

    render(<App snapshot={snapshot} commandFacade={commandFacade} />);

    expect(screen.getByText("Transcription recovered")).toBeInTheDocument();
    expect(screen.getByText("transcription worker was not running after app restart")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Cancel transcription" })).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Retry transcription" }));

    expect(calls).toEqual([{ method: "transcribeMeeting", args: { meetingId: "circuit-review" } }]);
  });

  it("blocks retrying a transcription job until the saved Whisper path has matching Test path evidence", async () => {
    const user = userEvent.setup();
    const calls: Array<{ method: string; args?: unknown }> = [];
    const snapshot = connectedSnapshot({
      model: {
        kind: "untested",
        configuredPath: "/models/ggml-base.en.bin",
      },
      transcriptionJob: {
        id: "transcription-circuit-review-1700000001000",
        kind: "Transcription",
        meetingId: "circuit-review",
        state: "Recovery",
        cancelRequested: false,
        startedAtMs: 1_700_000_001_000,
        lastError: "transcription worker was not running after app restart",
      },
    });
    const commandFacade = fakeCommandFacade({
      transcribeMeeting: async (args) => {
        calls.push({ method: "transcribeMeeting", args });
        return snapshot;
      },
    });

    render(<App snapshot={snapshot} commandFacade={commandFacade} />);

    const retry = screen.getByRole("button", { name: "Retry transcription" });
    expect(retry).toBeDisabled();
    expect(retry).toHaveAttribute(
      "title",
      "Run Test path for the saved Whisper model file before transcription.",
    );
    await user.click(retry);

    expect(calls).toEqual([]);
  });

  it("blocks retrying a transcription job for unsupported Whisper files with choose-model guidance", async () => {
    const user = userEvent.setup();
    const calls: Array<{ method: string; args?: unknown }> = [];
    const snapshot = unsupportedWhisperSnapshot("/models/extensionless", {
      transcriptionJob: {
        id: "transcription-circuit-review-1700000001000",
        kind: "Transcription",
        meetingId: "circuit-review",
        state: "Recovery",
        cancelRequested: false,
        startedAtMs: 1_700_000_001_000,
        lastError: "transcription worker was not running after app restart",
      },
    });
    const commandFacade = fakeCommandFacade({
      transcribeMeeting: async (args) => {
        calls.push({ method: "transcribeMeeting", args });
        return snapshot;
      },
    });

    render(<App snapshot={snapshot} commandFacade={commandFacade} />);

    const retry = screen.getByRole("button", { name: "Retry transcription" });
    expect(retry).toBeDisabled();
    expect(retry).toHaveAttribute(
      "title",
      "Choose a supported .bin or .gguf Whisper model file before transcription.",
    );
    await user.click(retry);

    expect(calls).toEqual([]);
  });

  it("retries the selected meeting's retryable summary job without a cancel control", async () => {
    const user = userEvent.setup();
    const calls: Array<{ method: string; args?: unknown }> = [];
    const snapshot = connectedSnapshot({
      summaryJob: {
        id: "summary-circuit-review-1700000002000",
        kind: "Summary",
        meetingId: "circuit-review",
        state: "Retry",
        cancelRequested: false,
        startedAtMs: 1_700_000_002_000,
      },
    });
    const commandFacade = fakeCommandFacade({
      generateSummary: async (args) => {
        calls.push({ method: "generateSummary", args });
        return snapshot;
      },
    });

    render(<App snapshot={snapshot} commandFacade={commandFacade} />);

    expect(screen.getByText("Summary retryable")).toBeInTheDocument();
    expect(screen.getByText("Retry this summary job when you are ready.")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Cancel summary" })).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Retry summary" }));

    expect(calls).toEqual([{ method: "generateSummary", args: { meetingId: "circuit-review" } }]);
  });

  it("keeps retry summary disabled when last Ollama test could not confirm availability", async () => {
    const user = userEvent.setup();
    const calls: Array<{ method: string; args?: unknown }> = [];
    const setupGuidance =
      "Ollama is unavailable: connection refused. Start Ollama with `ollama serve`, verify the local base URL, then run Test Ollama again. Availability is not checked in the background.";
    const snapshot = connectedSnapshot({
      setupGuidance: ollamaSetupGuidance({
        availability: "UnavailableAtLastTest",
        message: "Last explicit Test Ollama could not confirm local summary availability.",
        setupGuidance,
        lastConnectionTest: {
          baseUrl: "http://127.0.0.1:11434",
          requestedModel: "qwen3.6:27b",
          testedAtMs: 1_700_000_004_000,
          state: "Unavailable",
          selectedLocalModelTag: "qwen3.6:27b",
          installedLocalModels: null,
          pullCommand: null,
          failureDetail: "Ollama is unavailable: connection refused.",
        },
      }),
      summaryJob: {
        id: "summary-circuit-review-1700000002000",
        kind: "Summary",
        meetingId: "circuit-review",
        state: "Retry",
        cancelRequested: false,
        startedAtMs: 1_700_000_002_000,
      },
    });
    const commandFacade = fakeCommandFacade({
      generateSummary: async (args) => {
        calls.push({ method: "generateSummary", args });
        return snapshot;
      },
    });

    render(<App snapshot={snapshot} commandFacade={commandFacade} />);

    const retry = screen.getByRole("button", { name: "Retry summary" });
    expect(retry).toBeDisabled();
    expect(retry).toHaveAttribute("title", setupGuidance);
    await user.click(retry);

    expect(calls).toEqual([]);
  });

  it("keeps retry summary disabled when the selected meeting has no transcript segments", () => {
    const snapshot = connectedSnapshot({
      meetings: [
        {
          ...getMockDesktopSnapshot().meetings[0],
          segments: [],
          transcriptText: "",
          transcriptState: "Unavailable",
        },
      ],
      selectedMeetingId: "circuit-review",
      summaryJob: {
        id: "summary-circuit-review-1700000002000",
        kind: "Summary",
        meetingId: "circuit-review",
        state: "Retry",
        cancelRequested: false,
        startedAtMs: 1_700_000_002_000,
      },
    });

    render(<App snapshot={snapshot} commandFacade={fakeCommandFacade()} />);

    expect(screen.getByRole("button", { name: "Retry summary" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Retry summary" })).toHaveAttribute(
      "title",
      "Generate a transcript before requesting a summary.",
    );
  });

  it("does not show retry controls for another meeting's terminal retryable jobs", () => {
    const snapshot = connectedSnapshot({
      selectedMeetingId: "circuit-review",
      transcriptionJob: {
        id: "transcription-design-standup-1700000001000",
        kind: "Transcription",
        meetingId: "design-standup",
        state: "Recovery",
        cancelRequested: false,
        startedAtMs: 1_700_000_001_000,
      },
      summaryJob: {
        id: "summary-design-standup-1700000002000",
        kind: "Summary",
        meetingId: "design-standup",
        state: "Retry",
        cancelRequested: false,
        startedAtMs: 1_700_000_002_000,
      },
    });

    render(<App snapshot={snapshot} commandFacade={fakeCommandFacade()} />);

    expect(screen.queryByRole("button", { name: "Retry transcription" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Retry summary" })).not.toBeInTheDocument();
  });

  it("keeps cancel enabled for a running transcription job while the matching command is pending", async () => {
    const user = userEvent.setup();
    const calls: string[] = [];
    let finishTranscription!: () => void;
    const snapshot = connectedSnapshot({
      recording: {
        ...getMockDesktopSnapshot().recording,
        state: "Complete",
        recovery_action: "Previous local desktop WAV artifacts are saved.",
      },
      transcriptionJob: {
        id: "transcription-circuit-review-1700000001000",
        kind: "Transcription",
        meetingId: "circuit-review",
        state: "Running",
        cancelRequested: false,
        startedAtMs: 1_700_000_001_000,
      },
    });
    const pendingTranscription = new Promise<typeof snapshot>((resolve) => {
      finishTranscription = () => resolve(snapshot);
    });
    const commandFacade = fakeCommandFacade({
      transcribeMeeting: async () => {
        calls.push("transcribeMeeting");
        return pendingTranscription;
      },
      cancelTranscription: async () => {
        calls.push("cancelTranscription");
        return {
          ...snapshot,
          transcriptionJob: {
            ...snapshot.transcriptionJob!,
            state: "CancelRequested",
            cancelRequested: true,
          },
        };
      },
    });

    render(<App snapshot={snapshot} commandFacade={commandFacade} />);

    await user.click(screen.getByRole("button", { name: "Transcribe" }));
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Transcribing" })).toBeDisabled(),
    );
    expect(screen.getByRole("button", { name: "Cancel transcription" })).toBeEnabled();
    await user.click(screen.getByRole("button", { name: "Cancel transcription" }));

    await waitFor(() => expect(calls).toEqual(["transcribeMeeting", "cancelTranscription"]));
    await act(async () => {
      finishTranscription();
      await pendingTranscription;
    });
    expect(screen.getByText("Transcription cancel requested")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Cancel transcription" })).toBeDisabled();
  });

  it("polls the desktop snapshot while a command job is active", async () => {
    let pollCount = 0;
    const running = connectedSnapshot({
      transcriptionJob: {
        id: "transcription-circuit-review-1700000001000",
        kind: "Transcription",
        meetingId: "circuit-review",
        state: "Running",
        cancelRequested: false,
        startedAtMs: 1_700_000_001_000,
      },
    });
    const complete = connectedSnapshot({
      transcription: {
        meetingId: "circuit-review",
        state: "Complete",
        failure: null,
      },
      transcriptionJob: {
        ...running.transcriptionJob!,
        state: "Complete",
      },
    });
    const commandFacade = {
      ...fakeCommandFacade(),
      desktopSnapshot: async () => {
        pollCount += 1;
        return complete;
      },
    };

    render(<App snapshot={running} commandFacade={commandFacade} />);

    expect(screen.getByText("Transcription running")).toBeInTheDocument();
    await waitFor(
      () => {
        expect(pollCount).toBeGreaterThan(0);
        expect(screen.getByText("Transcription complete")).toBeInTheDocument();
      },
      { timeout: 1_000 },
    );
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
    const commandFacade = fakeCommandFacade({
      generateSummary: async () => returned,
    });

    render(<App snapshot={initial} commandFacade={commandFacade} />);

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

    render(<App snapshot={initial} commandFacade={fakeCommandFacade()} />);

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
        recovery_action: "Previous local desktop WAV artifacts are saved.",
      },
    });
    const returned = connectedSnapshot();
    const pendingSnapshot = new Promise<typeof returned>((resolve) => {
      finishCommand = () => resolve(returned);
    });
    const commandFacade = fakeCommandFacade({
      startRecording: async () => {
        calls.push("startRecording");
        return pendingSnapshot;
      },
    });

    render(<App snapshot={initial} commandFacade={commandFacade} />);

    await user.click(screen.getByRole("button", { name: "Start recording" }));

    expect(screen.getByRole("button", { name: "Starting recording" })).toBeDisabled();
    await user.click(screen.getByRole("button", { name: "Starting recording" }));
    expect(calls).toEqual(["startRecording"]);

    finishCommand();
    expect(await screen.findByRole("button", { name: "Stop recording" })).toBeEnabled();
  });

  it("shows invocation errors without replacing the current snapshot", async () => {
    const user = userEvent.setup();
    const initial = connectedSnapshot({
      recording: {
        ...getMockDesktopSnapshot().recording,
        state: "Complete",
        recovery_action: "Previous local desktop WAV artifacts are saved.",
        storage_location: { app_private_path: "meetings/circuit-review/audio" },
      },
    });
    const commandFacade = fakeCommandFacade({
      startRecording: async () => {
        throw new Error("microphone permission denied");
      },
    });

    render(<App snapshot={initial} commandFacade={commandFacade} />);

    await user.click(screen.getByRole("button", { name: "Start recording" }));

    expect(screen.getByRole("alert")).toHaveTextContent("microphone permission denied");
    expect(screen.getByRole("heading", { name: "Circuit Review" })).toBeInTheDocument();
    expect(screen.getAllByText("meetings/circuit-review/audio").length).toBeGreaterThan(0);
  });
});

function connectedSnapshot(overrides: Partial<ReturnType<typeof getMockDesktopSnapshot>> = {}) {
  const base = getMockDesktopSnapshot();
  const defaultWhisperPath = "~/Library/Application Support/Curiosity/models/base.en.bin";
  const settingsWhisperPath = overrides.settings?.whisperModelPath.trim();
  const configuredWhisperPath =
    overrides.model?.configuredPath.trim() || settingsWhisperPath || defaultWhisperPath;
  const model = overrides.model ?? {
    kind: "ready" as const,
    configuredPath: configuredWhisperPath,
  };
  const setupGuidance =
    overrides.setupGuidance ??
    (model.kind === "ready"
      ? whisperSetupGuidanceForPath(model.configuredPath, validWhisperPathTestEvidence(model.configuredPath))
      : model.kind === "untested"
        ? whisperSetupGuidanceForPath(model.configuredPath, null)
        : base.setupGuidance);

  return {
    ...base,
    commandSurface: {
      ready: true,
      detail: "Connected to local desktop commands.",
    },
    capture: {
      microphone: "Ready" as const,
      systemAudio: "SystemAudioUnavailable" as const,
    },
    model,
    setupGuidance,
    ...overrides,
  };
}

function validWhisperPathTestEvidence(path: string) {
  return {
    testedPath: path,
    testedAtMs: 1_700_000_001_000,
    state: "Valid" as const,
    fileSizeBytes: 16,
    sha256: "8b68af71d2eaaec61d5b4f50e330493cc0074323676962d9761cbc7c6810ba54",
    failureDetail: null,
  };
}

function whisperSetupGuidanceForPath(
  path: string,
  lastPathTest: ReturnType<typeof validWhisperPathTestEvidence> | null,
) {
  const base = getMockDesktopSnapshot();
  return {
    ...base.setupGuidance,
    whisper: {
      state: "ReadablePath" as const,
      configuredPath: path,
      message: "Whisper model path is readable; compatibility is not verified.",
      setupGuidance: "Use Test path for file evidence, then transcribe a sample to verify compatibility.",
      compatibilityNote: "Readability does not prove model compatibility.",
      lastPathTest,
      lastSuccessfulTranscription: null,
    },
  };
}

function ollamaSetupGuidance(
  overrides: Partial<ReturnType<typeof getMockDesktopSnapshot>["setupGuidance"]["ollama"]> = {},
) {
  const base = getMockDesktopSnapshot();
  return {
    ...base.setupGuidance,
    ollama: {
      state: "ConfiguredNotChecked" as const,
      baseUrl: "http://127.0.0.1:11434",
      model: "qwen3.6:27b",
      availability: "UnknownUntilTest" as const,
      message: "Ollama is configured for a local loopback URL and model.",
      setupGuidance: "Start Ollama manually, install the selected local model if needed, then run Test Ollama.",
      lastConnectionTest: null,
      ...overrides,
    },
  };
}

function unsupportedWhisperSnapshot(
  path: string,
  overrides: Partial<ReturnType<typeof getMockDesktopSnapshot>> = {},
) {
  return connectedSnapshot({
    model: {
      kind: "unsupported",
      configuredPath: path,
    },
    setupGuidance: {
      ...getMockDesktopSnapshot().setupGuidance,
      whisper: {
        state: "UnsupportedFile",
        configuredPath: path,
        message: "Whisper model path must use a supported .bin or .gguf file.",
        setupGuidance: "Choose an existing whisper.cpp-compatible .bin or .gguf model file.",
        compatibilityNote: "Test path only accepts .bin and .gguf model files.",
        lastPathTest: null,
        lastSuccessfulTranscription: null,
      },
    },
    ...overrides,
  });
}

function fakeCommandFacade(overrides: Partial<DesktopCommandFacade> = {}): DesktopCommandFacade {
  const snapshot = connectedSnapshot();
  return {
    desktopSnapshot: async () => snapshot,
    searchMeetings: async () => [],
    startRecording: async () => snapshot,
    importAudioFile: async () => snapshot,
    stopRecording: async () => snapshot,
    transcribeMeeting: async () => snapshot,
    correctTranscriptSegment: async () => snapshot,
    cancelTranscription: async () => snapshot,
    renameMeeting: async () => snapshot,
    exportMeetingJson: async () => snapshot,
    exportMeeting: async () => snapshot,
    deleteMeeting: async () => snapshot,
    generateSummary: async () => snapshot,
    cancelSummary: async () => snapshot,
    saveWhisperModelPath: async () => snapshot,
    saveAnalysisSettings: async () => snapshot,
    saveRawAudioRetentionPolicy: async () => snapshot,
    requestAppleCalendarAccess: async () => ({
      ...snapshot,
      calendarContext: {
        ...snapshot.calendarContext,
        permissionState: "Granted",
        availabilityState: "Ready",
        message: "Apple Calendar access is granted; no upcoming events found in the next 24 hours.",
      },
    }),
    attachCalendarEventContext: async () => snapshot,
    testWhisperModelPath: async () => ({
      state: "Valid",
      message: "Whisper model path is readable.",
      setupGuidance: "",
      fileSizeBytes: 16,
      sha256: "8b68af71d2eaaec61d5b4f50e330493cc0074323676962d9761cbc7c6810ba54",
    }),
    testOllamaConnection: async () => ({
      state: "Available",
      message: "Ollama is reachable.",
      setupGuidance: "",
      selectedLocalModelTag: "qwen3.6:27b",
      installedLocalModels: ["qwen3.6:27b"],
      pullCommand: null,
    }),
    ...overrides,
  };
}
