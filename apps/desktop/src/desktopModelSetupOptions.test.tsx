import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ComponentProps } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { DesktopModelSetupOptions, type DesktopModelSetupOptionsData } from "./desktopModelSetupOptions";

function modelSetupOptions(overrides: Partial<DesktopModelSetupOptionsData> = {}): DesktopModelSetupOptionsData {
  return {
    whisper: {
      mode: "ManualFile",
      title: "Local Whisper file",
      detail:
        "Choose an existing whisper.cpp-compatible .bin or .gguf model file. Curiosity does not download Whisper models yet.",
      chooseLabel: "Choose model",
      saveLabel: "Save Whisper",
      testLabel: "Test path",
      downloadsManaged: false,
      acceptedExtensions: ["bin", "gguf"],
    },
    ollama: {
      mode: "ManualOllama",
      title: "Local Ollama models",
      detail:
        "Start Ollama locally and install one of the listed local model tags manually before running Test Ollama.",
      automaticPulls: false,
      candidates: [
        {
          id: "ollama-qwen3-6-27b",
          displayName: "Qwen 3.6 27B",
          modelTag: "qwen3.6:27b",
          pullCommand: "ollama pull qwen3.6:27b",
          defaultCandidate: true,
          setupNotes: "Install Ollama locally, then run `ollama pull qwen3.6:27b`.",
        },
        {
          id: "ollama-gemma4-31b",
          displayName: "Gemma 4 31B",
          modelTag: "gemma4:31b",
          pullCommand: "ollama pull gemma4:31b",
          defaultCandidate: true,
          setupNotes: "Install Ollama locally, then run `ollama pull gemma4:31b`.",
        },
      ],
    },
    ...overrides,
  };
}

function renderSetupOptions(
  overrides: Partial<ComponentProps<typeof DesktopModelSetupOptions>> = {},
) {
  const props: ComponentProps<typeof DesktopModelSetupOptions> = {
    options: modelSetupOptions(),
    selectedOllamaModel: "qwen3.6:27b",
    settingsInputDisabled: false,
    copyPullCommandDisabled: false,
    onCopyPullCommand: vi.fn().mockResolvedValue(undefined),
    onChooseOllamaCandidate: vi.fn(),
    ...overrides,
  };

  return {
    ...render(<DesktopModelSetupOptions {...props} />),
    props,
  };
}

afterEach(() => {
  cleanup();
});

describe("DesktopModelSetupOptions", () => {
  it("renders the manual setup wrapper and Whisper option metadata", () => {
    const { container } = renderSetupOptions();
    const setupOptions = screen.getByLabelText("Manual model setup options");

    expect(container.firstElementChild).toBe(setupOptions);
    expect(setupOptions).toHaveClass("model-setup-options");
    expect(within(setupOptions).getByText("Local Whisper file")).toBeInTheDocument();
    expect(
      within(setupOptions).getByText(
        "Choose an existing whisper.cpp-compatible .bin or .gguf model file. Curiosity does not download Whisper models yet.",
      ),
    ).toBeInTheDocument();
    expect(within(setupOptions).getByText("Accepted: .bin, .gguf")).toHaveClass("setup-option-meta");
    expect(within(setupOptions).getByText("Managed downloads unavailable")).toHaveClass("setup-option-meta");
  });

  it("renders the Ollama option metadata and candidate pull commands", () => {
    renderSetupOptions();
    const setupOptions = screen.getByLabelText("Manual model setup options");

    expect(within(setupOptions).getByText("Local Ollama models")).toBeInTheDocument();
    expect(
      within(setupOptions).getByText(
        "Start Ollama locally and install one of the listed local model tags manually before running Test Ollama.",
      ),
    ).toBeInTheDocument();
    expect(within(setupOptions).getByText("Manual pulls only")).toHaveClass("setup-option-meta");
    expect(within(setupOptions).getByText("Qwen 3.6 27B")).toBeInTheDocument();
    expect(within(setupOptions).getByText("qwen3.6:27b")).toBeInTheDocument();
    expect(within(setupOptions).getByText("ollama pull qwen3.6:27b")).toHaveClass("setup-option-meta");
    expect(within(setupOptions).getByText("Gemma 4 31B")).toBeInTheDocument();
    expect(within(setupOptions).getByText("gemma4:31b")).toBeInTheDocument();
    expect(within(setupOptions).getByText("ollama pull gemma4:31b")).toHaveClass("setup-option-meta");
  });

  it("chooses the exact model tag from the selected candidate Use button", async () => {
    const user = userEvent.setup();
    const onChooseOllamaCandidate = vi.fn();
    renderSetupOptions({ onChooseOllamaCandidate });

    await user.click(screen.getAllByRole("button", { name: "Use" })[1]);

    expect(onChooseOllamaCandidate).toHaveBeenCalledTimes(1);
    expect(onChooseOllamaCandidate).toHaveBeenCalledWith("gemma4:31b");
  });

  it("disables Use for the selected candidate and when settings inputs are disabled", () => {
    const { rerender, props } = renderSetupOptions();

    const enabledButtons = screen.getAllByRole("button", { name: "Use" });
    expect(enabledButtons[0]).toBeDisabled();
    expect(enabledButtons[0]).toHaveAttribute("title", "Use this model tag in the local settings form.");
    expect(enabledButtons[1]).toBeEnabled();
    expect(enabledButtons[1]).toHaveAttribute("title", "Use this model tag in the local settings form.");

    rerender(<DesktopModelSetupOptions {...props} selectedOllamaModel="unlisted:latest" settingsInputDisabled />);

    for (const button of screen.getAllByRole("button", { name: "Use" })) {
      expect(button).toBeDisabled();
      expect(button).toHaveAttribute("title", "Use this model tag in the local settings form.");
    }
  });

  it("copies the exact candidate pull command and respects the disabled copy state", async () => {
    const user = userEvent.setup();
    const onCopyPullCommand = vi.fn().mockResolvedValue(undefined);
    const { rerender, props } = renderSetupOptions({ onCopyPullCommand });

    await user.click(screen.getByRole("button", { name: "Copy pull command for gemma4:31b" }));

    expect(onCopyPullCommand).toHaveBeenCalledTimes(1);
    expect(onCopyPullCommand).toHaveBeenCalledWith("ollama pull gemma4:31b");

    rerender(<DesktopModelSetupOptions {...props} copyPullCommandDisabled />);

    expect(screen.getByRole("button", { name: "Copy pull command for qwen3.6:27b" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Copy pull command for gemma4:31b" })).toBeDisabled();
  });

  it("renders enabled-state labels from setup options data", () => {
    const options = modelSetupOptions({
      whisper: {
        ...modelSetupOptions().whisper,
        downloadsManaged: true,
      },
      ollama: {
        ...modelSetupOptions().ollama,
        automaticPulls: true,
      },
    });

    renderSetupOptions({ options });

    expect(screen.getByText("Managed downloads enabled")).toBeInTheDocument();
    expect(screen.getByText("Automatic pulls enabled")).toBeInTheDocument();
  });
});
