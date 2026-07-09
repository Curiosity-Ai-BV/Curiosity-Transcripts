import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ComponentProps } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";

import type { PersistedRawAudioRetentionPolicy } from "./commandAdapter";
import { DesktopSettingsForm } from "./desktopSettingsForm";
import type { PendingCommand } from "./desktopWorkspaceState";

function renderSettingsForm(
  overrides: Partial<ComponentProps<typeof DesktopSettingsForm>> = {},
) {
  const props: ComponentProps<typeof DesktopSettingsForm> = {
    settingsForm: {
      whisperModelPath: "/Users/adrian/models/ggml-base.en.bin",
      ollamaBaseUrl: "http://127.0.0.1:11435",
      ollamaModel: "gemma4:31b",
      rawAudioRetentionPolicy: "Retain",
    },
    settingsFeedback: null,
    pendingCommand: null,
    settingsInputDisabled: false,
    settingsActionDisabled: false,
    chooseWhisperModelDisabled: false,
    chooseWhisperModelButtonTitle: "Choose a local Whisper model file.",
    testWhisperButtonTitle: "Test the configured Whisper path.",
    saveWhisperButtonTitle: "Save the configured Whisper path.",
    testOllamaButtonTitle: "Test the configured local Ollama server and model.",
    saveAnalysisButtonTitle: "Save local analysis settings.",
    saveRetentionButtonTitle: "Save default raw-audio retention.",
    copyPullCommandDisabled: false,
    onWhisperModelPathChange: vi.fn(),
    onChooseWhisperModel: vi.fn(),
    onTestWhisperModelPath: vi.fn(),
    onSaveWhisperModelPath: vi.fn(),
    onOllamaBaseUrlChange: vi.fn(),
    onOllamaModelChange: vi.fn(),
    onTestOllamaConnection: vi.fn(),
    onSaveAnalysisSettings: vi.fn(),
    onRawAudioRetentionPolicyChange: vi.fn(),
    onSaveRawAudioRetentionPolicy: vi.fn(),
    onCopyPullCommand: vi.fn().mockResolvedValue(undefined),
    ...overrides,
  };

  return {
    ...render(<DesktopSettingsForm {...props} />),
    props,
  };
}

afterEach(() => {
  cleanup();
});

describe("DesktopSettingsForm", () => {
  it("renders the local settings wrapper, form labels, placeholders, retention options, and supplied values", () => {
    const { container } = renderSettingsForm({
      settingsForm: {
        whisperModelPath: "/models/custom.gguf",
        ollamaBaseUrl: "http://127.0.0.1:11436",
        ollamaModel: "qwen3.6:27b",
        rawAudioRetentionPolicy: "DeleteAfterTranscription",
      },
    });

    const form = screen.getByLabelText("Local settings");
    expect(container.firstElementChild).toBe(form);
    expect(form).toHaveClass("settings-form");

    expect(screen.getByLabelText("Whisper model path")).toHaveValue("/models/custom.gguf");
    expect(screen.getByLabelText("Whisper model path")).toHaveAttribute(
      "placeholder",
      "/absolute/path/to/ggml-base.en.bin",
    );
    expect(screen.getByLabelText("Ollama base URL")).toHaveValue("http://127.0.0.1:11436");
    expect(screen.getByLabelText("Ollama base URL")).toHaveAttribute(
      "placeholder",
      "http://127.0.0.1:11434",
    );
    expect(screen.getByLabelText("Ollama model")).toHaveValue("qwen3.6:27b");
    expect(screen.getByLabelText("Ollama model")).toHaveAttribute("placeholder", "qwen3.6:27b");

    const retention = screen.getByLabelText("Raw audio retention");
    expect(retention).toHaveValue("DeleteAfterTranscription");
    expect(screen.getByRole("option", { name: "Retain" })).toHaveValue("Retain");
    expect(screen.getByRole("option", { name: "Delete after transcription" })).toHaveValue(
      "DeleteAfterTranscription",
    );
  });

  it("propagates controlled input and select changes with exact values", async () => {
    const onWhisperModelPathChange = vi.fn();
    const onOllamaBaseUrlChange = vi.fn();
    const onOllamaModelChange = vi.fn();
    const onRawAudioRetentionPolicyChange = vi.fn<(value: PersistedRawAudioRetentionPolicy) => void>();
    renderSettingsForm({
      onWhisperModelPathChange,
      onOllamaBaseUrlChange,
      onOllamaModelChange,
      onRawAudioRetentionPolicyChange,
    });

    fireEvent.change(screen.getByLabelText("Whisper model path"), { target: { value: "/tmp/model.gguf" } });
    fireEvent.change(screen.getByLabelText("Ollama base URL"), {
      target: { value: "http://127.0.0.1:11434" },
    });
    fireEvent.change(screen.getByLabelText("Ollama model"), { target: { value: "qwen3.6:27b" } });
    fireEvent.change(screen.getByLabelText("Raw audio retention"), {
      target: { value: "DeleteAfterTranscription" },
    });

    expect(onWhisperModelPathChange).toHaveBeenLastCalledWith("/tmp/model.gguf");
    expect(onOllamaBaseUrlChange).toHaveBeenLastCalledWith("http://127.0.0.1:11434");
    expect(onOllamaModelChange).toHaveBeenLastCalledWith("qwen3.6:27b");
    expect(onRawAudioRetentionPolicyChange).toHaveBeenCalledWith("DeleteAfterTranscription");
  });

  it("calls the matching action callbacks from Choose, Test, and Save buttons", async () => {
    const user = userEvent.setup();
    const onChooseWhisperModel = vi.fn();
    const onTestWhisperModelPath = vi.fn();
    const onSaveWhisperModelPath = vi.fn();
    const onTestOllamaConnection = vi.fn();
    const onSaveAnalysisSettings = vi.fn();
    const onSaveRawAudioRetentionPolicy = vi.fn();
    renderSettingsForm({
      onChooseWhisperModel,
      onTestWhisperModelPath,
      onSaveWhisperModelPath,
      onTestOllamaConnection,
      onSaveAnalysisSettings,
      onSaveRawAudioRetentionPolicy,
    });

    await user.click(screen.getByRole("button", { name: "Choose model" }));
    await user.click(screen.getByRole("button", { name: "Test path" }));
    await user.click(screen.getByRole("button", { name: "Save Whisper" }));
    await user.click(screen.getByRole("button", { name: "Test Ollama" }));
    await user.click(screen.getByRole("button", { name: "Save analysis" }));
    await user.click(screen.getByRole("button", { name: "Save retention" }));

    expect(onChooseWhisperModel).toHaveBeenCalledTimes(1);
    expect(onTestWhisperModelPath).toHaveBeenCalledTimes(1);
    expect(onSaveWhisperModelPath).toHaveBeenCalledTimes(1);
    expect(onTestOllamaConnection).toHaveBeenCalledTimes(1);
    expect(onSaveAnalysisSettings).toHaveBeenCalledTimes(1);
    expect(onSaveRawAudioRetentionPolicy).toHaveBeenCalledTimes(1);
  });

  it("applies disabled flags and App-supplied titles to inputs, buttons, and select controls", () => {
    renderSettingsForm({
      settingsInputDisabled: true,
      settingsActionDisabled: true,
      chooseWhisperModelDisabled: true,
      chooseWhisperModelButtonTitle: "Choose unavailable.",
      testWhisperButtonTitle: "Test Whisper unavailable.",
      saveWhisperButtonTitle: "Save Whisper unavailable.",
      testOllamaButtonTitle: "Test Ollama unavailable.",
      saveAnalysisButtonTitle: "Save analysis unavailable.",
      saveRetentionButtonTitle: "Save retention unavailable.",
    });

    expect(screen.getByLabelText("Whisper model path")).toBeDisabled();
    expect(screen.getByLabelText("Ollama base URL")).toBeDisabled();
    expect(screen.getByLabelText("Ollama model")).toBeDisabled();
    expect(screen.getByLabelText("Raw audio retention")).toBeDisabled();
    expect(screen.getByRole("button", { name: "Choose model" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Choose model" })).toHaveAttribute("title", "Choose unavailable.");
    expect(screen.getByRole("button", { name: "Test path" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Test path" })).toHaveAttribute(
      "title",
      "Test Whisper unavailable.",
    );
    expect(screen.getByRole("button", { name: "Save Whisper" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Save Whisper" })).toHaveAttribute(
      "title",
      "Save Whisper unavailable.",
    );
    expect(screen.getByRole("button", { name: "Test Ollama" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Test Ollama" })).toHaveAttribute(
      "title",
      "Test Ollama unavailable.",
    );
    expect(screen.getByRole("button", { name: "Save analysis" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Save analysis" })).toHaveAttribute(
      "title",
      "Save analysis unavailable.",
    );
    expect(screen.getByRole("button", { name: "Save retention" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Save retention" })).toHaveAttribute(
      "title",
      "Save retention unavailable.",
    );
  });

  it.each([
    ["choose-whisper-model", "Choosing model"],
    ["test-whisper", "Testing path"],
    ["save-whisper", "Saving Whisper"],
    ["test-ollama", "Testing Ollama"],
    ["save-analysis", "Saving analysis"],
    ["save-retention", "Saving retention"],
  ] satisfies Array<[Exclude<PendingCommand, null>, string]>)(
    "swaps the pending label for %s",
    (pendingCommand, pendingLabel) => {
      renderSettingsForm({ pendingCommand });

      expect(screen.getByRole("button", { name: pendingLabel })).toBeInTheDocument();
    },
  );

  it("renders App-supplied settings feedback and delegates pull-command copying", async () => {
    const user = userEvent.setup();
    const onCopyPullCommand = vi.fn().mockResolvedValue(undefined);
    renderSettingsForm({
      settingsFeedback: {
        tone: "blocked",
        message: "Selected local model is missing.",
        metadata: {
          kind: "ollama",
          selectedLocalModelTag: "qwen3.6:27b",
          installedLocalModels: ["gemma4:31b"],
          pullCommand: "ollama pull qwen3.6:27b",
        },
      },
      onCopyPullCommand,
    });

    const feedback = screen.getByRole("status");
    expect(within(feedback).getByText("Selected local model is missing.")).toBeInTheDocument();
    expect(within(feedback).getByText("Pull command: ollama pull qwen3.6:27b")).toBeInTheDocument();

    await user.click(within(feedback).getByRole("button", { name: "Copy pull command for qwen3.6:27b" }));

    expect(onCopyPullCommand).toHaveBeenCalledTimes(1);
    expect(onCopyPullCommand).toHaveBeenCalledWith("ollama pull qwen3.6:27b");
  });

  it("does not render the retired NeverSave raw-audio retention option", () => {
    renderSettingsForm();

    expect(screen.queryByRole("option", { name: "Never save" })).not.toBeInTheDocument();
    expect(screen.queryByRole("option", { name: "NeverSave" })).not.toBeInTheDocument();
  });
});
