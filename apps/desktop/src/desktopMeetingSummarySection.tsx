import { FileText, ShieldCheck } from "@phosphor-icons/react";

import type { StatusView } from "./commandAdapter";
import { StatusLine } from "./desktopWorkspaceComponents";
import type { PendingCommand } from "./desktopWorkspaceState";

export interface MeetingSummarySectionProps {
  disclosure: StatusView;
  summaryText: string | null;
  summaryDisabled: boolean;
  summaryButtonTitle: string;
  pendingCommand: PendingCommand;
  onGenerateSummary(): void;
}

export function MeetingSummarySection({
  disclosure,
  summaryText,
  summaryDisabled,
  summaryButtonTitle,
  pendingCommand,
  onGenerateSummary,
}: MeetingSummarySectionProps) {
  return (
    <section className="summary-section" aria-label="Structured summary">
      <div>
        <h3>Structured summary</h3>
        <StatusLine
          icon={<ShieldCheck size={18} weight="regular" />}
          label={disclosure.label}
          value={disclosure.detail}
          tone={disclosure.tone}
        />
        {summaryText ? <p className="summary-text">{summaryText}</p> : null}
      </div>
      <button
        type="button"
        className="button"
        disabled={summaryDisabled}
        title={summaryButtonTitle}
        onClick={onGenerateSummary}
      >
        <FileText size={16} weight="regular" />
        {pendingCommand === "summary" ? "Generating summary" : "Generate summary"}
      </button>
    </section>
  );
}
