import { Trash } from "@phosphor-icons/react";

import type { StatusView } from "./commandAdapter";

export interface SummaryFailureOutcome {
  message: string;
  setupGuidance?: string | null;
}

export interface DesktopCommandOutcomesProps {
  commandError: string | null;
  showExportOutcome: boolean;
  exportCommandState: StatusView;
  showDeleteOutcome: boolean;
  deleteCommandState: StatusView;
  failedDeleteMeetingId?: string;
  retryDeleteDisabled: boolean;
  retryDeleteTitle: string;
  summaryFailure: SummaryFailureOutcome | null;
  onRetryDelete(): void;
}

export function DesktopCommandOutcomes({
  commandError,
  showExportOutcome,
  exportCommandState,
  showDeleteOutcome,
  deleteCommandState,
  failedDeleteMeetingId,
  retryDeleteDisabled,
  retryDeleteTitle,
  summaryFailure,
  onRetryDelete,
}: DesktopCommandOutcomesProps) {
  return (
    <>
      {commandError ? (
        <p role="alert" className="command-error">
          {commandError}
        </p>
      ) : null}
      {showExportOutcome ? (
        <p role="status" className={`command-outcome ${exportCommandState.tone}`}>
          <strong>{exportCommandState.label}</strong>
          <span>{exportCommandState.detail}</span>
        </p>
      ) : null}
      {showDeleteOutcome ? (
        <div role="status" className={`command-outcome ${deleteCommandState.tone}`}>
          <strong>{deleteCommandState.label}</strong>
          <span>{deleteCommandState.detail}</span>
          {failedDeleteMeetingId ? (
            <button
              type="button"
              className="button danger"
              disabled={retryDeleteDisabled}
              title={retryDeleteTitle}
              onClick={onRetryDelete}
            >
              <Trash size={16} weight="regular" />
              Retry delete
            </button>
          ) : null}
        </div>
      ) : null}
      {summaryFailure ? (
        <div role="status" className="command-outcome blocked">
          <strong>{summaryFailure.message}</strong>
          {summaryFailure.setupGuidance ? <span>{summaryFailure.setupGuidance}</span> : null}
        </div>
      ) : null}
    </>
  );
}
