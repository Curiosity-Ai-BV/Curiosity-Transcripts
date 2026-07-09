import type { Tone } from "./commandAdapter";
import { CopyPullCommandButton } from "./desktopWorkspaceComponents";

export interface SettingsFeedback {
  tone: Tone;
  message: string;
  metadata?:
    | {
        kind: "whisper";
        fileSizeBytes: number;
        sha256: string;
      }
    | {
        kind: "ollama";
        selectedLocalModelTag: string | null;
        installedLocalModels: string[] | null;
        pullCommand: string | null;
      };
}

interface DesktopSettingsFeedbackProps {
  feedback: SettingsFeedback | null;
  copyPullCommandDisabled: boolean;
  onCopyPullCommand(pullCommand: string): Promise<void>;
}

export function DesktopSettingsFeedback({
  feedback,
  copyPullCommandDisabled,
  onCopyPullCommand,
}: DesktopSettingsFeedbackProps) {
  if (!feedback) {
    return null;
  }

  return (
    <div className={`settings-feedback ${feedback.tone}`} role="status">
      <span>{feedback.message}</span>
      {feedback.metadata ? (
        <span className="settings-feedback-metadata">
          {feedback.metadata.kind === "whisper" ? (
            <>
              <span>Size: {feedback.metadata.fileSizeBytes} bytes</span>
              <span>SHA-256: {feedback.metadata.sha256}</span>
            </>
          ) : (
            <>
              {feedback.metadata.selectedLocalModelTag ? (
                <span>Selected model: {feedback.metadata.selectedLocalModelTag}</span>
              ) : null}
              {feedback.metadata.installedLocalModels ? (
                <span>
                  Installed models:{" "}
                  {feedback.metadata.installedLocalModels.length > 0
                    ? feedback.metadata.installedLocalModels.join(", ")
                    : "none reported"}
                </span>
              ) : null}
              {feedback.metadata.pullCommand ? (
                <span className="pull-command-copy">
                  <span>Pull command: {feedback.metadata.pullCommand}</span>
                  <CopyPullCommandButton
                    pullCommand={feedback.metadata.pullCommand}
                    disabled={copyPullCommandDisabled}
                    onCopy={onCopyPullCommand}
                  />
                </span>
              ) : null}
            </>
          )}
        </span>
      ) : null}
    </div>
  );
}
