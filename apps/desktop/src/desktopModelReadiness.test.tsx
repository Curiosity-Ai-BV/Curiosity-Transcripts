import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";

import type { DesktopSnapshot } from "./commandAdapter";
import { DesktopModelReadiness, type DesktopModelReadinessProps } from "./desktopModelReadiness";

type WhisperGuidance = DesktopSnapshot["setupGuidance"]["whisper"];
type OllamaGuidance = DesktopSnapshot["setupGuidance"]["ollama"];

function whisperGuidance(overrides: Partial<WhisperGuidance> = {}): WhisperGuidance {
  return {
    state: "ReadablePath",
    configuredPath: "/models/ggml-base.en.bin",
    message: "Whisper model path is readable; compatibility is not verified.",
    setupGuidance: "Use Test path for file evidence, then transcribe a sample to verify compatibility.",
    compatibilityNote: "Readability does not prove model compatibility.",
    lastPathTest: null,
    lastSuccessfulTranscription: null,
    ...overrides,
  };
}

function ollamaGuidance(overrides: Partial<OllamaGuidance> = {}): OllamaGuidance {
  return {
    state: "ConfiguredNotChecked",
    baseUrl: "http://127.0.0.1:11434",
    model: "qwen3.6:27b",
    availability: "UnknownUntilTest",
    message: "Ollama is configured for a local loopback URL and model.",
    setupGuidance:
      "Start Ollama manually, install the selected local model if needed, then run Test Ollama. Availability is unknown until Test Ollama runs.",
    lastConnectionTest: null,
    ...overrides,
  };
}

function modelReadinessProps(
  overrides: Partial<DesktopModelReadinessProps> = {},
): DesktopModelReadinessProps {
  return {
    whisper: whisperGuidance(),
    whisperLabel: "Whisper path readable",
    whisperTone: "warn",
    ollama: ollamaGuidance(),
    ollamaLabel: "Ollama availability unknown",
    ollamaTone: "warn",
    copyPullCommandDisabled: false,
    onCopyPullCommand: vi.fn(),
    ...overrides,
  };
}

afterEach(() => {
  cleanup();
});

describe("DesktopModelReadiness", () => {
  it("renders the base Whisper and Ollama readiness items with App-supplied labels and tones", () => {
    const { container } = render(<DesktopModelReadiness {...modelReadinessProps()} />);
    const readiness = screen.getByLabelText("Model readiness guidance");

    expect(container.firstElementChild).toBe(readiness);
    expect(readiness).toHaveClass("model-readiness");

    const items = readiness.querySelectorAll(".readiness-item");
    expect(items).toHaveLength(2);
    expect(items[0]).toHaveClass("warn");
    expect(items[1]).toHaveClass("warn");

    const whisperItem = items[0] as HTMLElement;
    expect(within(whisperItem).getByText("Whisper path readable")).toHaveClass("status-pill", "warn");
    expect(
      within(whisperItem).getByText("Whisper model path is readable; compatibility is not verified."),
    ).toBeInTheDocument();
    expect(within(whisperItem).getByText("/models/ggml-base.en.bin")).toHaveClass("readiness-path");
    expect(
      within(whisperItem).getByText("Use Test path for file evidence, then transcribe a sample to verify compatibility."),
    ).toBeInTheDocument();
    expect(within(whisperItem).getByText("Readability does not prove model compatibility.")).toBeInTheDocument();

    const ollamaItem = items[1] as HTMLElement;
    expect(within(ollamaItem).getByText("Ollama availability unknown")).toHaveClass("status-pill", "warn");
    expect(within(ollamaItem).getByText("Ollama is configured for a local loopback URL and model.")).toBeInTheDocument();
    expect(within(ollamaItem).getByText("http://127.0.0.1:11434 / qwen3.6:27b")).toHaveClass(
      "readiness-path",
    );
    expect(
      within(ollamaItem).getByText(
        "Start Ollama manually, install the selected local model if needed, then run Test Ollama. Availability is unknown until Test Ollama runs.",
      ),
    ).toBeInTheDocument();
  });

  it("omits optional evidence blocks when evidence is null", () => {
    render(<DesktopModelReadiness {...modelReadinessProps()} />);

    expect(document.querySelector(".readiness-evidence")).not.toBeInTheDocument();
    expect(screen.queryByText(/Last explicit Test path:/)).not.toBeInTheDocument();
    expect(screen.queryByText(/Last successful transcription at/)).not.toBeInTheDocument();
    expect(screen.queryByText(/Last explicit Test Ollama:/)).not.toBeInTheDocument();
  });

  it("renders Whisper path-test evidence with formatted timestamp, tested-path fallback, optional fields, and failure detail", () => {
    const { rerender } = render(
      <DesktopModelReadiness
        {...modelReadinessProps({
          whisper: whisperGuidance({
            lastPathTest: {
              state: "Invalid",
              testedAtMs: Date.UTC(2026, 6, 9, 10, 11, 12),
              testedPath: "",
              fileSizeBytes: null,
              sha256: null,
              failureDetail: "File could not be read.",
            },
          }),
        })}
      />,
    );

    expect(screen.getByText("Last explicit Test path: Invalid at 2026-07-09T10:11:12.000Z")).toBeInTheDocument();
    expect(screen.getByText("Tested path: none")).toBeInTheDocument();
    expect(screen.queryByText(/Size:/)).not.toBeInTheDocument();
    expect(screen.queryByText(/SHA-256:/)).not.toBeInTheDocument();
    expect(screen.getByText("File could not be read.")).toBeInTheDocument();

    rerender(
      <DesktopModelReadiness
        {...modelReadinessProps({
          whisper: whisperGuidance({
            lastPathTest: {
              state: "Valid",
              testedAtMs: Date.UTC(2026, 6, 9, 10, 11, 12),
              testedPath: "/models/valid.bin",
              fileSizeBytes: 123456,
              sha256: "abc123",
              failureDetail: null,
            },
          }),
        })}
      />,
    );

    expect(screen.getByText("Tested path: /models/valid.bin")).toBeInTheDocument();
    expect(screen.getByText("Size: 123456 bytes")).toBeInTheDocument();
    expect(screen.getByText("SHA-256: abc123")).toBeInTheDocument();
    expect(screen.queryByText("File could not be read.")).not.toBeInTheDocument();
  });

  it("renders last successful transcription evidence with singular and plural segment labels", () => {
    const { rerender } = render(
      <DesktopModelReadiness
        {...modelReadinessProps({
          whisper: whisperGuidance({
            lastSuccessfulTranscription: {
              usedAtMs: Date.UTC(2026, 6, 9, 11, 12, 13),
              modelPath: "/models/ggml-base.en.bin",
              provider: "whisper.cpp",
              modelName: "base.en",
              meetingId: "meeting-1",
              modelRunId: "run-1",
              transcriptVersionId: "version-1",
              segmentCount: 1,
              fileSizeBytes: 7654321,
              modifiedAtMs: Date.UTC(2026, 6, 8, 9, 10, 11),
            },
          }),
        })}
      />,
    );

    expect(screen.getByText("Last successful transcription at 2026-07-09T11:12:13.000Z")).toBeInTheDocument();
    expect(screen.getByText("Model path: /models/ggml-base.en.bin")).toBeInTheDocument();
    expect(screen.getByText("Provider: whisper.cpp")).toBeInTheDocument();
    expect(screen.getByText("Model: base.en")).toBeInTheDocument();
    expect(screen.getByText("Meeting: meeting-1")).toBeInTheDocument();
    expect(screen.getByText("Model run: run-1")).toBeInTheDocument();
    expect(screen.getByText("Transcript version: version-1")).toBeInTheDocument();
    expect(screen.getByText("Transcript: 1 segment")).toBeInTheDocument();
    expect(screen.getByText("Model file size: 7654321 bytes")).toBeInTheDocument();
    expect(screen.getByText("Model modified: 2026-07-08T09:10:11.000Z")).toBeInTheDocument();

    rerender(
      <DesktopModelReadiness
        {...modelReadinessProps({
          whisper: whisperGuidance({
            lastSuccessfulTranscription: {
              usedAtMs: Date.UTC(2026, 6, 9, 11, 12, 13),
              modelPath: "/models/ggml-base.en.bin",
              provider: "whisper.cpp",
              modelName: "base.en",
              meetingId: "meeting-1",
              modelRunId: "run-1",
              transcriptVersionId: "version-1",
              segmentCount: 2,
              fileSizeBytes: 7654321,
              modifiedAtMs: Date.UTC(2026, 6, 8, 9, 10, 11),
            },
          }),
        })}
      />,
    );

    expect(screen.getByText("Transcript: 2 segments")).toBeInTheDocument();
  });

  it("renders Ollama connection evidence with populated and empty observed-model labels", () => {
    const { rerender } = render(
      <DesktopModelReadiness
        {...modelReadinessProps({
          ollama: ollamaGuidance({
            lastConnectionTest: {
              state: "Unavailable",
              testedAtMs: Date.UTC(2026, 6, 9, 12, 13, 14),
              baseUrl: "http://localhost:11434",
              requestedModel: "qwen3.6:27b",
              selectedLocalModelTag: "qwen3.6:27b",
              installedLocalModels: ["qwen3.6:27b", "gemma4:31b"],
              pullCommand: "ollama pull qwen3.6:27b",
              failureDetail: "Ollama refused the request.",
            },
          }),
        })}
      />,
    );

    expect(screen.getByText("Last explicit Test Ollama: Unavailable at 2026-07-09T12:13:14.000Z")).toBeInTheDocument();
    expect(screen.getByText("Request: http://localhost:11434 / qwen3.6:27b")).toBeInTheDocument();
    expect(screen.getByText("Selected model: qwen3.6:27b")).toBeInTheDocument();
    expect(screen.getByText("Observed models: qwen3.6:27b, gemma4:31b")).toBeInTheDocument();
    expect(screen.getByText("Pull command: ollama pull qwen3.6:27b")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Copy pull command for qwen3.6:27b" })).toBeInTheDocument();
    expect(screen.getByText("Ollama refused the request.")).toBeInTheDocument();
    expect(screen.getByText("Last explicit observation, not current availability.")).toBeInTheDocument();

    rerender(
      <DesktopModelReadiness
        {...modelReadinessProps({
          ollama: ollamaGuidance({
            lastConnectionTest: {
              state: "Available",
              testedAtMs: Date.UTC(2026, 6, 9, 12, 13, 14),
              baseUrl: "http://localhost:11434",
              requestedModel: "qwen3.6:27b",
              selectedLocalModelTag: null,
              installedLocalModels: [],
              pullCommand: null,
              failureDetail: null,
            },
          }),
        })}
      />,
    );

    expect(screen.queryByText(/Selected model:/)).not.toBeInTheDocument();
    expect(screen.getByText("Observed models: none reported")).toBeInTheDocument();
    expect(screen.queryByText(/Pull command:/)).not.toBeInTheDocument();
    expect(screen.queryByText("Ollama refused the request.")).not.toBeInTheDocument();
  });

  it("copies the exact Ollama pull command and respects the disabled state", async () => {
    const user = userEvent.setup();
    const onCopyPullCommand = vi.fn().mockResolvedValue(undefined);
    const { rerender } = render(
      <DesktopModelReadiness
        {...modelReadinessProps({
          ollama: ollamaGuidance({
            lastConnectionTest: {
              state: "Unavailable",
              testedAtMs: Date.UTC(2026, 6, 9, 12, 13, 14),
              baseUrl: "http://localhost:11434",
              requestedModel: "gemma4:31b",
              selectedLocalModelTag: null,
              installedLocalModels: null,
              pullCommand: "ollama pull gemma4:31b",
              failureDetail: null,
            },
          }),
          onCopyPullCommand,
        })}
      />,
    );

    await user.click(screen.getByRole("button", { name: "Copy pull command for gemma4:31b" }));
    expect(onCopyPullCommand).toHaveBeenCalledTimes(1);
    expect(onCopyPullCommand).toHaveBeenCalledWith("ollama pull gemma4:31b");

    rerender(
      <DesktopModelReadiness
        {...modelReadinessProps({
          ollama: ollamaGuidance({
            lastConnectionTest: {
              state: "Unavailable",
              testedAtMs: Date.UTC(2026, 6, 9, 12, 13, 14),
              baseUrl: "http://localhost:11434",
              requestedModel: "gemma4:31b",
              selectedLocalModelTag: null,
              installedLocalModels: null,
              pullCommand: "ollama pull gemma4:31b",
              failureDetail: null,
            },
          }),
          copyPullCommandDisabled: true,
          onCopyPullCommand,
        })}
      />,
    );

    expect(screen.getByRole("button", { name: "Copy pull command for gemma4:31b" })).toBeDisabled();
  });
});
