import { FileText, PencilSimple } from "@phosphor-icons/react";

import type { MeetingView } from "./commandAdapter";
import { StatusPill } from "./desktopWorkspaceComponents";
import type { PendingCommand } from "./desktopWorkspaceState";

type MeetingDetailHeaderMeeting = Pick<MeetingView, "startedAt" | "title" | "transcriptState">;

export interface MeetingDetailHeaderProps {
  meeting: MeetingDetailHeaderMeeting;
  renameTitle: string;
  renameInputDisabled: boolean;
  renameDisabled: boolean;
  transcribeDisabled: boolean;
  renameButtonTitle: string;
  transcribeButtonTitle: string;
  pendingCommand: PendingCommand;
  onRenameTitleChange(value: string): void;
  onRename(): void;
  onTranscribe(): void;
}

export function MeetingDetailHeader({
  meeting,
  renameTitle,
  renameInputDisabled,
  renameDisabled,
  transcribeDisabled,
  renameButtonTitle,
  transcribeButtonTitle,
  pendingCommand,
  onRenameTitleChange,
  onRename,
  onTranscribe,
}: MeetingDetailHeaderProps) {
  return (
    <div className="detail-header">
      <div>
        <p className="eyebrow">{meeting.startedAt}</p>
        <h2>{meeting.title}</h2>
        <div className="rename-title-row">
          <label className="rename-title-field" htmlFor="selected-meeting-title">
            <span>Selected meeting title</span>
            <input
              id="selected-meeting-title"
              value={renameTitle}
              onChange={(event) => onRenameTitleChange(event.target.value)}
              disabled={renameInputDisabled}
            />
          </label>
          <button
            type="button"
            className="button"
            disabled={renameDisabled}
            title={renameButtonTitle}
            onClick={onRename}
          >
            <PencilSimple size={16} weight="regular" />
            {pendingCommand === "rename" ? "Renaming" : "Rename"}
          </button>
        </div>
      </div>
      <div className="detail-header-actions">
        <StatusPill tone={meeting.transcriptState === "Ready" ? "ready" : "active"} label={meeting.transcriptState} />
        <button
          type="button"
          className="button primary"
          disabled={transcribeDisabled}
          title={transcribeButtonTitle}
          onClick={onTranscribe}
        >
          <FileText size={16} weight="regular" />
          {pendingCommand === "transcribe" ? "Transcribing" : "Transcribe"}
        </button>
      </div>
    </div>
  );
}
