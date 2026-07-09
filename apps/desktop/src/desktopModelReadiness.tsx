import type { DesktopSnapshot, Tone } from "./commandAdapter";
import { CopyPullCommandButton, StatusPill } from "./desktopWorkspaceComponents";
import { formatEvidenceTimestamp } from "./desktopWorkspaceState";

export interface DesktopModelReadinessProps {
  whisper: DesktopSnapshot["setupGuidance"]["whisper"];
  whisperLabel: string;
  whisperTone: Tone;
  ollama: DesktopSnapshot["setupGuidance"]["ollama"];
  ollamaLabel: string;
  ollamaTone: Tone;
  copyPullCommandDisabled: boolean;
  onCopyPullCommand(pullCommand: string): Promise<void>;
}

export function DesktopModelReadiness({
  whisper,
  whisperLabel,
  whisperTone,
  ollama,
  ollamaLabel,
  ollamaTone,
  copyPullCommandDisabled,
  onCopyPullCommand,
}: DesktopModelReadinessProps) {
  return (
    <div className="model-readiness" aria-label="Model readiness guidance">
      <div className={`readiness-item ${whisperTone}`}>
        <div className="readiness-heading">
          <StatusPill tone={whisperTone} label={whisperLabel} />
        </div>
        <p>{whisper.message}</p>
        {whisper.configuredPath ? <span className="readiness-path">{whisper.configuredPath}</span> : null}
        <p>{whisper.setupGuidance}</p>
        <small>{whisper.compatibilityNote}</small>
        {whisper.lastPathTest ? (
          <div className="readiness-evidence">
            <strong>
              Last explicit Test path: {whisper.lastPathTest.state} at{" "}
              {formatEvidenceTimestamp(whisper.lastPathTest.testedAtMs)}
            </strong>
            <span>Tested path: {whisper.lastPathTest.testedPath || "none"}</span>
            {whisper.lastPathTest.fileSizeBytes !== null ? (
              <span>Size: {whisper.lastPathTest.fileSizeBytes} bytes</span>
            ) : null}
            {whisper.lastPathTest.sha256 ? <span>SHA-256: {whisper.lastPathTest.sha256}</span> : null}
            {whisper.lastPathTest.failureDetail ? <span>{whisper.lastPathTest.failureDetail}</span> : null}
          </div>
        ) : null}
        {whisper.lastSuccessfulTranscription ? (
          <div className="readiness-evidence">
            <strong>
              Last successful transcription at{" "}
              {formatEvidenceTimestamp(whisper.lastSuccessfulTranscription.usedAtMs)}
            </strong>
            <span>Model path: {whisper.lastSuccessfulTranscription.modelPath}</span>
            <span>Provider: {whisper.lastSuccessfulTranscription.provider}</span>
            <span>Model: {whisper.lastSuccessfulTranscription.modelName}</span>
            <span>Meeting: {whisper.lastSuccessfulTranscription.meetingId}</span>
            <span>Model run: {whisper.lastSuccessfulTranscription.modelRunId}</span>
            <span>Transcript version: {whisper.lastSuccessfulTranscription.transcriptVersionId}</span>
            <span>
              Transcript: {whisper.lastSuccessfulTranscription.segmentCount} segment
              {whisper.lastSuccessfulTranscription.segmentCount === 1 ? "" : "s"}
            </span>
            <span>Model file size: {whisper.lastSuccessfulTranscription.fileSizeBytes} bytes</span>
            <span>
              Model modified: {formatEvidenceTimestamp(whisper.lastSuccessfulTranscription.modifiedAtMs)}
            </span>
          </div>
        ) : null}
      </div>
      <div className={`readiness-item ${ollamaTone}`}>
        <div className="readiness-heading">
          <StatusPill tone={ollamaTone} label={ollamaLabel} />
        </div>
        <p>{ollama.message}</p>
        <span className="readiness-path">
          {ollama.baseUrl} / {ollama.model}
        </span>
        <p>{ollama.setupGuidance}</p>
        {ollama.lastConnectionTest ? (
          <div className="readiness-evidence">
            <strong>
              Last explicit Test Ollama: {ollama.lastConnectionTest.state} at{" "}
              {formatEvidenceTimestamp(ollama.lastConnectionTest.testedAtMs)}
            </strong>
            <span>
              Request: {ollama.lastConnectionTest.baseUrl} / {ollama.lastConnectionTest.requestedModel}
            </span>
            {ollama.lastConnectionTest.selectedLocalModelTag ? (
              <span>Selected model: {ollama.lastConnectionTest.selectedLocalModelTag}</span>
            ) : null}
            {ollama.lastConnectionTest.installedLocalModels ? (
              <span>
                Observed models:{" "}
                {ollama.lastConnectionTest.installedLocalModels.length > 0
                  ? ollama.lastConnectionTest.installedLocalModels.join(", ")
                  : "none reported"}
              </span>
            ) : null}
            {ollama.lastConnectionTest.pullCommand ? (
              <span className="pull-command-copy">
                <span>Pull command: {ollama.lastConnectionTest.pullCommand}</span>
                <CopyPullCommandButton
                  pullCommand={ollama.lastConnectionTest.pullCommand}
                  disabled={copyPullCommandDisabled}
                  onCopy={onCopyPullCommand}
                />
              </span>
            ) : null}
            {ollama.lastConnectionTest.failureDetail ? (
              <span>{ollama.lastConnectionTest.failureDetail}</span>
            ) : null}
            <small>Last explicit observation, not current availability.</small>
          </div>
        ) : null}
      </div>
    </div>
  );
}
