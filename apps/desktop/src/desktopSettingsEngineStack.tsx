import {
  CalendarBlank,
  CheckCircle,
  FileText,
  Microphone,
  ShieldCheck,
  WarningDiamond,
} from "@phosphor-icons/react";

import type { Tone } from "./commandAdapter";
import type { PendingCommand } from "./desktopWorkspaceState";
import { StatusLine } from "./desktopWorkspaceComponents";

export interface EngineStatusLine {
  label: string;
  value: string;
  tone: Tone;
}

export interface DesktopSettingsEngineStackProps {
  model: EngineStatusLine;
  transcription: EngineStatusLine;
  transcriptionJob: EngineStatusLine | null;
  summaryJob: EngineStatusLine | null;
  microphone: EngineStatusLine;
  systemAudio: EngineStatusLine;
  calendar: EngineStatusLine;
  selectedMeetingAnalysis: EngineStatusLine | null;
  canCancelTranscriptionJob: boolean;
  canRetryTranscriptionJob: boolean;
  canCancelSummaryJob: boolean;
  canRetrySummaryJob: boolean;
  cancelTranscriptionDisabled: boolean;
  retryTranscriptionDisabled: boolean;
  cancelSummaryDisabled: boolean;
  retrySummaryDisabled: boolean;
  cancelTranscriptionButtonTitle: string;
  retryTranscriptionButtonTitle: string;
  cancelSummaryButtonTitle: string;
  summaryButtonTitle: string;
  pendingCommand: PendingCommand;
  onCancelTranscriptionJob(): void;
  onRetryTranscriptionJob(): void;
  onCancelSummaryJob(): void;
  onRetrySummaryJob(): void;
}

export function DesktopSettingsEngineStack({
  model,
  transcription,
  transcriptionJob,
  summaryJob,
  microphone,
  systemAudio,
  calendar,
  selectedMeetingAnalysis,
  canCancelTranscriptionJob,
  canRetryTranscriptionJob,
  canCancelSummaryJob,
  canRetrySummaryJob,
  cancelTranscriptionDisabled,
  retryTranscriptionDisabled,
  cancelSummaryDisabled,
  retrySummaryDisabled,
  cancelTranscriptionButtonTitle,
  retryTranscriptionButtonTitle,
  cancelSummaryButtonTitle,
  summaryButtonTitle,
  pendingCommand,
  onCancelTranscriptionJob,
  onRetryTranscriptionJob,
  onCancelSummaryJob,
  onRetrySummaryJob,
}: DesktopSettingsEngineStackProps) {
  return (
    <div className="engine-stack" aria-label="Model and capture status">
      <StatusLine
        icon={<CheckCircle size={18} weight="regular" />}
        label={model.label}
        value={model.value}
        tone={model.tone}
      />
      <StatusLine
        icon={<FileText size={18} weight="regular" />}
        label={transcription.label}
        value={transcription.value}
        tone={transcription.tone}
      />
      {transcriptionJob ? (
        <>
          <StatusLine
            icon={<FileText size={18} weight="regular" />}
            label={transcriptionJob.label}
            value={transcriptionJob.value}
            tone={transcriptionJob.tone}
          />
          {canCancelTranscriptionJob ? (
            <button
              type="button"
              className="button"
              disabled={cancelTranscriptionDisabled}
              title={cancelTranscriptionButtonTitle}
              onClick={onCancelTranscriptionJob}
            >
              {pendingCommand === "cancel-transcription" ? "Canceling transcription" : "Cancel transcription"}
            </button>
          ) : null}
          {canRetryTranscriptionJob ? (
            <button
              type="button"
              className="button"
              disabled={retryTranscriptionDisabled}
              title={retryTranscriptionButtonTitle}
              onClick={onRetryTranscriptionJob}
            >
              {pendingCommand === "transcribe" ? "Retrying transcription" : "Retry transcription"}
            </button>
          ) : null}
        </>
      ) : null}
      {summaryJob ? (
        <>
          <StatusLine
            icon={<FileText size={18} weight="regular" />}
            label={summaryJob.label}
            value={summaryJob.value}
            tone={summaryJob.tone}
          />
          {canCancelSummaryJob ? (
            <button
              type="button"
              className="button"
              disabled={cancelSummaryDisabled}
              title={cancelSummaryButtonTitle}
              onClick={onCancelSummaryJob}
            >
              {pendingCommand === "cancel-summary" ? "Canceling summary" : "Cancel summary"}
            </button>
          ) : null}
          {canRetrySummaryJob ? (
            <button
              type="button"
              className="button"
              disabled={retrySummaryDisabled}
              title={summaryButtonTitle}
              onClick={onRetrySummaryJob}
            >
              {pendingCommand === "summary" ? "Retrying summary" : "Retry summary"}
            </button>
          ) : null}
        </>
      ) : null}
      <StatusLine
        icon={<Microphone size={18} weight="regular" />}
        label={microphone.label}
        value={microphone.value}
        tone={microphone.tone}
      />
      <StatusLine
        icon={<WarningDiamond size={18} weight="regular" />}
        label={systemAudio.label}
        value={systemAudio.value}
        tone={systemAudio.tone}
      />
      <StatusLine
        icon={<CalendarBlank size={18} weight="regular" />}
        label={calendar.label}
        value={calendar.value}
        tone={calendar.tone}
      />
      {selectedMeetingAnalysis ? (
        <StatusLine
          icon={<ShieldCheck size={18} weight="regular" />}
          label={selectedMeetingAnalysis.label}
          value={selectedMeetingAnalysis.value}
          tone={selectedMeetingAnalysis.tone}
        />
      ) : null}
    </div>
  );
}
