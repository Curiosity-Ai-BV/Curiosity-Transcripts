import { CopySimple } from "@phosphor-icons/react";
import type { ReactNode } from "react";

import type { Tone } from "./commandAdapter";
import { ollamaPullCommandModelLabel } from "./desktopWorkspaceState";

export function StatusPill({ tone, label }: { tone: Tone; label: string }) {
  return <span className={`status-pill ${tone}`}>{label}</span>;
}

export function CopyPullCommandButton({
  pullCommand,
  disabled,
  onCopy,
}: {
  pullCommand: string;
  disabled: boolean;
  onCopy(pullCommand: string): Promise<void>;
}) {
  const modelLabel = ollamaPullCommandModelLabel(pullCommand);
  return (
    <button
      type="button"
      className="button quiet pull-command-copy-button"
      disabled={disabled}
      title="Copy this pull command to the clipboard."
      aria-label={`Copy pull command for ${modelLabel}`}
      onClick={() => {
        void onCopy(pullCommand);
      }}
    >
      <CopySimple size={14} weight="regular" />
      Copy
    </button>
  );
}

export function StatusLine({
  icon,
  label,
  value,
  tone,
}: {
  icon: ReactNode;
  label: string;
  value: string;
  tone: Tone;
}) {
  return (
    <div className={`status-line ${tone}`}>
      <span className="status-icon">{icon}</span>
      <span>
        <strong>{label}</strong>
        <small>{value}</small>
      </span>
    </div>
  );
}

export function IconFrame({ children, tone }: { children: ReactNode; tone: Tone }) {
  return <span className={`icon-frame ${tone}`}>{children}</span>;
}

export function SkeletonList() {
  return (
    <div className="skeleton-list" aria-label="Loading workspace">
      <p>Loading workspace</p>
      <span />
      <span />
      <span />
    </div>
  );
}
