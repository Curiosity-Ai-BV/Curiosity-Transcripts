import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ComponentProps } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { DesktopSettingsFeedback, type SettingsFeedback } from "./desktopSettingsFeedback";

function renderSettingsFeedback(
  overrides: Partial<ComponentProps<typeof DesktopSettingsFeedback>> = {},
) {
  const props: ComponentProps<typeof DesktopSettingsFeedback> = {
    feedback: {
      tone: "ready",
      message: "Settings are ready.",
    },
    copyPullCommandDisabled: false,
    onCopyPullCommand: vi.fn().mockResolvedValue(undefined),
    ...overrides,
  };

  return {
    ...render(<DesktopSettingsFeedback {...props} />),
    props,
  };
}

afterEach(() => {
  cleanup();
});

describe("DesktopSettingsFeedback", () => {
  it("renders nothing when App has no feedback to show", () => {
    const { container } = renderSettingsFeedback({ feedback: null });

    expect(container.firstElementChild).toBeNull();
  });

  it("renders the status wrapper, tone class, and App-owned message", () => {
    const { container } = renderSettingsFeedback({
      feedback: {
        tone: "blocked",
        message: "Ollama is unavailable.",
      },
    });

    const feedback = screen.getByRole("status");
    expect(container.firstElementChild).toBe(feedback);
    expect(feedback).toHaveClass("settings-feedback", "blocked");
    expect(within(feedback).getByText("Ollama is unavailable.")).toBeInTheDocument();
  });

  it("renders Whisper metadata so the chosen local model can be audited", () => {
    renderSettingsFeedback({
      feedback: {
        tone: "ready",
        message: "Whisper model path is valid.",
        metadata: {
          kind: "whisper",
          fileSizeBytes: 734003200,
          sha256: "8b68af71d2eaaec61d5b4f50e330493cc0074323676962d9761cbc7c6810ba54",
        },
      },
    });

    const feedback = screen.getByRole("status");
    expect(within(feedback).getByText("Size: 734003200 bytes")).toBeInTheDocument();
    expect(
      within(feedback).getByText("SHA-256: 8b68af71d2eaaec61d5b4f50e330493cc0074323676962d9761cbc7c6810ba54"),
    ).toBeInTheDocument();
  });

  it("renders Ollama metadata and delegates pull-command copying", async () => {
    const user = userEvent.setup();
    const onCopyPullCommand = vi.fn().mockResolvedValue(undefined);
    renderSettingsFeedback({
      feedback: {
        tone: "blocked",
        message: "Selected local model is missing.",
        metadata: {
          kind: "ollama",
          selectedLocalModelTag: "qwen3.6:27b",
          installedLocalModels: ["gemma4:31b", "llama3.2:latest"],
          pullCommand: "ollama pull qwen3.6:27b",
        },
      },
      onCopyPullCommand,
    });

    const feedback = screen.getByRole("status");
    expect(within(feedback).getByText("Selected model: qwen3.6:27b")).toBeInTheDocument();
    expect(within(feedback).getByText("Installed models: gemma4:31b, llama3.2:latest")).toBeInTheDocument();
    expect(within(feedback).getByText("Pull command: ollama pull qwen3.6:27b")).toBeInTheDocument();

    await user.click(within(feedback).getByRole("button", { name: "Copy pull command for qwen3.6:27b" }));

    expect(onCopyPullCommand).toHaveBeenCalledTimes(1);
    expect(onCopyPullCommand).toHaveBeenCalledWith("ollama pull qwen3.6:27b");
  });

  it("renders empty installed models as none reported", () => {
    renderSettingsFeedback({
      feedback: {
        tone: "blocked",
        message: "Selected local model is missing.",
        metadata: {
          kind: "ollama",
          selectedLocalModelTag: null,
          installedLocalModels: [],
          pullCommand: null,
        },
      },
    });

    expect(screen.getByText("Installed models: none reported")).toBeInTheDocument();
  });

  it("omits optional Ollama metadata fields when App passes null", () => {
    renderSettingsFeedback({
      feedback: {
        tone: "ready",
        message: "Ollama connection is available.",
        metadata: {
          kind: "ollama",
          selectedLocalModelTag: null,
          installedLocalModels: null,
          pullCommand: null,
        },
      } satisfies SettingsFeedback,
    });

    const feedback = screen.getByRole("status");
    expect(feedback).not.toHaveTextContent("Selected model:");
    expect(feedback).not.toHaveTextContent("Installed models:");
    expect(feedback).not.toHaveTextContent("Pull command:");
    expect(within(feedback).queryByRole("button", { name: /Copy pull command/ })).not.toBeInTheDocument();
  });

  it("respects App's disabled copy state", () => {
    renderSettingsFeedback({
      feedback: {
        tone: "blocked",
        message: "Selected local model is missing.",
        metadata: {
          kind: "ollama",
          selectedLocalModelTag: "qwen3.6:27b",
          installedLocalModels: null,
          pullCommand: "ollama pull qwen3.6:27b",
        },
      },
      copyPullCommandDisabled: true,
    });

    expect(screen.getByRole("button", { name: "Copy pull command for qwen3.6:27b" })).toBeDisabled();
  });
});
