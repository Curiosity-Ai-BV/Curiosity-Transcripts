import {
  Moon,
  Sun,
  Waveform,
} from "@phosphor-icons/react";

export interface DesktopTopbarProps {
  appVersion: string;
  isLightTheme: boolean;
  themeButtonLabel: string;
  onToggleTheme(): void;
}

export function DesktopTopbar({
  appVersion,
  isLightTheme,
  themeButtonLabel,
  onToggleTheme,
}: DesktopTopbarProps) {
  return (
    <header className="topbar">
      <div className="brand-lockup">
        <span className="brand-mark" aria-hidden="true">
          <Waveform size={22} weight="fill" />
        </span>
        <div>
          <p className="eyebrow">Curiosity Transcripts</p>
          <h1>Transcript workspace</h1>
        </div>
      </div>
      <div className="topbar-controls" aria-label="Workspace controls">
        <span className="version-badge" aria-label={`Version ${appVersion}`}>
          v{appVersion}
        </span>
        <button
          type="button"
          className="theme-toggle"
          aria-label={themeButtonLabel}
          aria-pressed={isLightTheme}
          title={themeButtonLabel}
          onClick={onToggleTheme}
        >
          {isLightTheme ? <Moon size={16} weight="regular" /> : <Sun size={16} weight="regular" />}
          <span>{isLightTheme ? "Dark" : "Light"}</span>
        </button>
      </div>
    </header>
  );
}
