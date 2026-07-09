import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import {
  CopyPullCommandButton,
  IconFrame,
  SkeletonList,
  StatusLine,
  StatusPill,
} from "./desktopWorkspaceComponents";

describe("desktop workspace presentational components", () => {
  it("renders status pill text with the tone class", () => {
    render(<StatusPill tone="ready" label="Whisper path readable" />);

    const pill = screen.getByText("Whisper path readable");

    expect(pill).toHaveClass("status-pill", "ready");
  });

  it("renders status line label, value, and tone class", () => {
    const { container } = render(
      <StatusLine
        icon={<span aria-hidden="true">icon</span>}
        label="Private storage"
        value="meetings/circuit-review/audio"
        tone="warn"
      />,
    );

    expect(screen.getByText("Private storage")).toBeInTheDocument();
    expect(screen.getByText("meetings/circuit-review/audio")).toBeInTheDocument();
    expect(container.querySelector(".status-line")).toHaveClass("status-line", "warn");
  });

  it("renders icon frame children with the tone class", () => {
    render(
      <IconFrame tone="active">
        <span>Recording icon</span>
      </IconFrame>,
    );

    const child = screen.getByText("Recording icon");

    expect(child).toBeInTheDocument();
    expect(child.parentElement).toHaveClass("icon-frame", "active");
  });

  it("renders skeleton loading label and text", () => {
    render(<SkeletonList />);

    expect(screen.getByLabelText("Loading workspace")).toHaveClass("skeleton-list");
    expect(screen.getByText("Loading workspace")).toBeInTheDocument();
  });

  it("copies the original pull command from a button labelled by model tag", async () => {
    const user = userEvent.setup();
    const onCopy = vi.fn().mockResolvedValue(undefined);
    const pullCommand = "  ollama   pull   qwen3.6:27b  ";

    render(<CopyPullCommandButton pullCommand={pullCommand} disabled={false} onCopy={onCopy} />);

    await user.click(screen.getByRole("button", { name: "Copy pull command for qwen3.6:27b" }));

    expect(onCopy).toHaveBeenCalledTimes(1);
    expect(onCopy).toHaveBeenCalledWith(pullCommand);
  });
});
