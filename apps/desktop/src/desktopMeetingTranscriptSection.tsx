import { CheckCircle, PencilSimple } from "@phosphor-icons/react";

import type { TranscriptSegment } from "./commandAdapter";
import { formatTime } from "./desktopWorkspaceState";
import type { PendingCommand } from "./desktopWorkspaceState";

export interface MeetingTranscriptSectionProps {
  segments: TranscriptSegment[];
  editingSegmentId: string | null;
  segmentDraft: string;
  segmentDraftDisabled: boolean;
  correctionDisabled: boolean;
  saveCorrectionTitle: string;
  cancelCorrectionDisabled: boolean;
  editSegmentDisabled: boolean;
  editSegmentTitle: string;
  pendingCommand: PendingCommand;
  onSegmentDraftChange(value: string): void;
  onEditSegment(segmentId: string, text: string): void;
  onCancelCorrection(): void;
  onSaveCorrection(): void;
}

export function MeetingTranscriptSection({
  segments,
  editingSegmentId,
  segmentDraft,
  segmentDraftDisabled,
  correctionDisabled,
  saveCorrectionTitle,
  cancelCorrectionDisabled,
  editSegmentDisabled,
  editSegmentTitle,
  pendingCommand,
  onSegmentDraftChange,
  onEditSegment,
  onCancelCorrection,
  onSaveCorrection,
}: MeetingTranscriptSectionProps) {
  return (
    <section className="transcript-section">
      <h3>Transcript</h3>
      <div className="segments">
        {segments.map((segment) => {
          const isEditingSegment = editingSegmentId === segment.id;
          const showOriginalText = Boolean(
            segment.originalText && segment.originalText !== segment.text,
          );

          return (
            <article key={segment.id} className="segment">
              <time>{formatTime(segment.startMs)}</time>
              <div className="segment-body">
                {isEditingSegment ? (
                  <div className="segment-editor">
                    <label className="segment-editor-field">
                      <span>Transcript segment text</span>
                      <textarea
                        value={segmentDraft}
                        onChange={(event) => onSegmentDraftChange(event.target.value)}
                        disabled={segmentDraftDisabled}
                      />
                    </label>
                    <div className="segment-editor-actions">
                      <button
                        type="button"
                        className="button primary"
                        disabled={correctionDisabled}
                        title={saveCorrectionTitle}
                        onClick={onSaveCorrection}
                      >
                        <CheckCircle size={16} weight="regular" />
                        {pendingCommand === "correct-segment" ? "Saving correction" : "Save correction"}
                      </button>
                      <button
                        type="button"
                        className="button quiet"
                        disabled={cancelCorrectionDisabled}
                        onClick={onCancelCorrection}
                      >
                        Cancel correction
                      </button>
                    </div>
                  </div>
                ) : (
                  <>
                    <p>{segment.text}</p>
                    {showOriginalText ? (
                      <small className="segment-original">Original: {segment.originalText}</small>
                    ) : null}
                  </>
                )}
              </div>
              <span className="segment-channel">{segment.sourceChannel}</span>
              {isEditingSegment ? null : (
                <button
                  type="button"
                  className="button quiet segment-edit-button"
                  disabled={editSegmentDisabled}
                  title={editSegmentTitle}
                  onClick={() => onEditSegment(segment.id, segment.text)}
                >
                  <PencilSimple size={16} weight="regular" />
                  Edit segment
                </button>
              )}
            </article>
          );
        })}
      </div>
    </section>
  );
}
