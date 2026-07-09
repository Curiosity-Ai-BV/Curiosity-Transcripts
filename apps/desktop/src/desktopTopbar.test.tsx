import { Moon, Sun, Waveform } from "@phosphor-icons/react";
import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ComponentProps } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { DesktopTopbar } from "./desktopTopbar";

function renderDesktopTopbar(overrides: Partial<ComponentProps<typeof DesktopTopbar>> = {}) {
  const props: ComponentProps<typeof DesktopTopbar> = {
    appVersion: "2.3.4",
    isLightTheme: false,
    themeButtonLabel: "Switch to light mode",
    onToggleTheme: vi.fn(),
    ...overrides,
  };

  return {
    ...render(<DesktopTopbar {...props} />),
    props,
  };
}

function firstSvgPath(container: ParentNode) {
  const path = container.querySelector("svg path");
  expect(path).toBeInTheDocument();
  return path?.getAttribute("d");
}

afterEach(() => {
  cleanup();
});

describe("DesktopTopbar", () => {
  it("renders the brand lockup, workspace heading, and accessible version badge", () => {
    const referenceWaveformIcon = render(<Waveform size={22} weight="fill" />);
    const waveformIconPath = firstSvgPath(referenceWaveformIcon.container);
    referenceWaveformIcon.unmount();

    const { container } = renderDesktopTopbar();

    const topbar = container.querySelector("header.topbar");
    expect(topbar).toBeInTheDocument();

    const brandLockup = topbar?.querySelector(".brand-lockup");
    expect(brandLockup).toBeInTheDocument();

    const brandMark = brandLockup?.querySelector(".brand-mark");
    expect(brandMark).toBeInTheDocument();
    expect(brandMark).toHaveAttribute("aria-hidden", "true");
    expect(firstSvgPath(brandMark as HTMLElement)).toBe(waveformIconPath);

    const eyebrow = within(brandLockup as HTMLElement).getByText("Curiosity Transcripts");
    expect(eyebrow.tagName).toBe("P");
    expect(eyebrow).toHaveClass("eyebrow");
    expect(screen.getByRole("heading", { name: "Transcript workspace" }).tagName).toBe("H1");

    const controls = screen.getByLabelText("Workspace controls");
    expect(controls).toHaveClass("topbar-controls");

    const versionBadge = within(controls).getByLabelText("Version 2.3.4");
    expect(versionBadge).toHaveClass("version-badge");
    expect(versionBadge).toHaveTextContent("v2.3.4");
  });

  it("renders dark-theme controls with the Sun icon, Light label, and light-mode switch label", () => {
    const referenceSunIcon = render(<Sun size={16} weight="regular" />);
    const sunIconPath = firstSvgPath(referenceSunIcon.container);
    referenceSunIcon.unmount();

    renderDesktopTopbar({
      isLightTheme: false,
      themeButtonLabel: "Switch to light mode",
    });

    const button = screen.getByRole("button", { name: "Switch to light mode" });
    expect(button).toHaveClass("theme-toggle");
    expect(button).toHaveAttribute("title", "Switch to light mode");
    expect(button).toHaveAttribute("aria-pressed", "false");
    expect(within(button).getByText("Light")).toBeInTheDocument();
    expect(firstSvgPath(button)).toBe(sunIconPath);
  });

  it("renders light-theme controls with the Moon icon, Dark label, and dark-mode switch label", () => {
    const referenceMoonIcon = render(<Moon size={16} weight="regular" />);
    const moonIconPath = firstSvgPath(referenceMoonIcon.container);
    referenceMoonIcon.unmount();

    renderDesktopTopbar({
      isLightTheme: true,
      themeButtonLabel: "Switch to dark mode",
    });

    const button = screen.getByRole("button", { name: "Switch to dark mode" });
    expect(button).toHaveClass("theme-toggle");
    expect(button).toHaveAttribute("title", "Switch to dark mode");
    expect(button).toHaveAttribute("aria-pressed", "true");
    expect(within(button).getByText("Dark")).toBeInTheDocument();
    expect(firstSvgPath(button)).toBe(moonIconPath);
  });

  it("delegates theme toggles to App exactly once", async () => {
    const user = userEvent.setup();
    const onToggleTheme = vi.fn();
    renderDesktopTopbar({ onToggleTheme });

    await user.click(screen.getByRole("button", { name: "Switch to light mode" }));

    expect(onToggleTheme).toHaveBeenCalledTimes(1);
  });
});
