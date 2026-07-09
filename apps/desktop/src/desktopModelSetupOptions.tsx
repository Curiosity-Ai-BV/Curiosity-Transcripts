import type { DesktopSnapshot } from "./commandAdapter";
import { CopyPullCommandButton } from "./desktopWorkspaceComponents";

type SnapshotModelSetupOptions = DesktopSnapshot["modelSetupOptions"];

export interface DesktopModelSetupOptionsData {
  whisper: Omit<SnapshotModelSetupOptions["whisper"], "downloadsManaged"> & {
    downloadsManaged: boolean;
  };
  ollama: Omit<SnapshotModelSetupOptions["ollama"], "automaticPulls"> & {
    automaticPulls: boolean;
  };
}

export interface DesktopModelSetupOptionsProps {
  options: DesktopModelSetupOptionsData;
  selectedOllamaModel: string;
  settingsInputDisabled: boolean;
  copyPullCommandDisabled: boolean;
  onCopyPullCommand(pullCommand: string): Promise<void>;
  onChooseOllamaCandidate(modelTag: string): void;
}

export function DesktopModelSetupOptions({
  options,
  selectedOllamaModel,
  settingsInputDisabled,
  copyPullCommandDisabled,
  onCopyPullCommand,
  onChooseOllamaCandidate,
}: DesktopModelSetupOptionsProps) {
  return (
    <div className="model-setup-options" aria-label="Manual model setup options">
      <div className="setup-option-group">
        <strong>{options.whisper.title}</strong>
        <p>{options.whisper.detail}</p>
        <span className="setup-option-meta">
          Accepted: {options.whisper.acceptedExtensions.map((extension) => `.${extension}`).join(", ")}
        </span>
        <span className="setup-option-meta">
          {options.whisper.downloadsManaged ? "Managed downloads enabled" : "Managed downloads unavailable"}
        </span>
      </div>
      <div className="setup-option-group">
        <strong>{options.ollama.title}</strong>
        <p>{options.ollama.detail}</p>
        <span className="setup-option-meta">
          {options.ollama.automaticPulls ? "Automatic pulls enabled" : "Manual pulls only"}
        </span>
        <div className="ollama-candidate-list">
          {options.ollama.candidates.map((candidate) => (
            <div key={candidate.id} className="ollama-candidate-row">
              <span>
                <strong>{candidate.displayName}</strong>
                <small>{candidate.modelTag}</small>
              </span>
              <span className="pull-command-copy">
                <span className="setup-option-meta">{candidate.pullCommand}</span>
                <CopyPullCommandButton
                  pullCommand={candidate.pullCommand}
                  disabled={copyPullCommandDisabled}
                  onCopy={onCopyPullCommand}
                />
              </span>
              <button
                type="button"
                className="button"
                disabled={settingsInputDisabled || selectedOllamaModel === candidate.modelTag}
                title="Use this model tag in the local settings form."
                onClick={() => onChooseOllamaCandidate(candidate.modelTag)}
              >
                Use
              </button>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
