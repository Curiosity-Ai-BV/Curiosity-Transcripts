import { DownloadSimple, Trash } from "@phosphor-icons/react";

import type { ExportFormat } from "./commandAdapter";
import type { PendingCommand } from "./desktopWorkspaceState";

export interface MeetingDetailActionsProps {
  selectedExportFormat: ExportFormat;
  selectedExportFormatLabel: string;
  exportDisabled: boolean;
  deleteDisabled: boolean;
  exportButtonTitle: string;
  deleteButtonTitle: string;
  pendingCommand: PendingCommand;
  onExportFormatChange(format: ExportFormat): void;
  onExport(): void;
  onDelete(): void;
}

export function MeetingDetailActions({
  selectedExportFormat,
  selectedExportFormatLabel,
  exportDisabled,
  deleteDisabled,
  exportButtonTitle,
  deleteButtonTitle,
  pendingCommand,
  onExportFormatChange,
  onExport,
  onDelete,
}: MeetingDetailActionsProps) {
  return (
    <div className="detail-actions">
      <label className="export-format-field">
        <span>Export format</span>
        <select
          value={selectedExportFormat}
          disabled={exportDisabled}
          onChange={(event) => onExportFormatChange(event.target.value as ExportFormat)}
        >
          <option value="json">JSON</option>
          <option value="markdown">Markdown</option>
          <option value="srt">SRT</option>
        </select>
      </label>
      <button
        type="button"
        className="button"
        disabled={exportDisabled}
        title={exportButtonTitle}
        onClick={onExport}
      >
        <DownloadSimple size={16} weight="regular" />
        {pendingCommand === "export"
          ? `Exporting ${selectedExportFormatLabel}`
          : `Export ${selectedExportFormatLabel}`}
      </button>
      <button
        type="button"
        className="button danger"
        disabled={deleteDisabled}
        title={deleteButtonTitle}
        onClick={onDelete}
      >
        <Trash size={16} weight="regular" />
        {pendingCommand === "delete" ? "Deleting private data" : "Delete private data"}
      </button>
    </div>
  );
}
