import { CalendarBlank, FileText, ShieldCheck, Trash, Waveform } from "@phosphor-icons/react";

import type { StatusView } from "./commandAdapter";
import { StatusLine } from "./desktopWorkspaceComponents";

export interface MeetingPrivacyRowProps {
  storage: {
    label: string;
    path: string;
  };
  rawAudioRetention: StatusView | null;
  localProcessing: StatusView | null;
  calendarContext: string | null;
  exportState: StatusView;
  deleteState: StatusView;
}

export function MeetingPrivacyRow({
  storage,
  rawAudioRetention,
  localProcessing,
  calendarContext,
  exportState,
  deleteState,
}: MeetingPrivacyRowProps) {
  return (
    <div className="privacy-row" aria-label="Meeting privacy data state">
      <StatusLine icon={<ShieldCheck size={18} weight="regular" />} label={storage.label} value={storage.path} tone="ready" />
      {rawAudioRetention ? (
        <StatusLine icon={<Waveform size={18} weight="regular" />} label={rawAudioRetention.label} value={rawAudioRetention.detail} tone={rawAudioRetention.tone} />
      ) : null}
      {localProcessing ? (
        <StatusLine icon={<ShieldCheck size={18} weight="regular" />} label={localProcessing.label} value={localProcessing.detail} tone={localProcessing.tone} />
      ) : null}
      {calendarContext !== null ? (
        <StatusLine
          icon={<CalendarBlank size={18} weight="regular" />}
          label="Calendar context"
          value={calendarContext}
          tone="ready"
        />
      ) : null}
      <StatusLine icon={<FileText size={18} weight="regular" />} label={exportState.label} value={exportState.detail} tone={exportState.tone} />
      <StatusLine icon={<Trash size={18} weight="regular" />} label={deleteState.label} value={deleteState.detail} tone={deleteState.tone} />
    </div>
  );
}
