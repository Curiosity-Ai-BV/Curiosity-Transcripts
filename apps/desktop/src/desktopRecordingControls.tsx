import { FileText, FolderOpen, Microphone, Waveform } from "@phosphor-icons/react";

import type { StatusView } from "./commandAdapter";
import { IconFrame, StatusPill } from "./desktopWorkspaceComponents";
import type { PendingCommand } from "./desktopWorkspaceState";

export interface RecordingControlsProps {
  recording: StatusView;
  recordingTitle: string;
  importWavPath: string;
  recordingTitleDisabled: boolean;
  importWavPathDisabled: boolean;
  chooseWavDisabled: boolean;
  startDisabled: boolean;
  importDisabled: boolean;
  stopDisabled: boolean;
  chooseWavButtonTitle: string;
  startButtonTitle: string;
  importButtonTitle: string;
  stopButtonTitle: string;
  storagePath: string;
  pendingCommand: PendingCommand;
  onRecordingTitleChange(value: string): void;
  onImportWavPathChange(value: string): void;
  onChooseWav(): void;
  onStartRecording(): void;
  onImportWav(): void;
  onStopRecording(): void;
}

export function RecordingControls({
  recording,
  recordingTitle,
  importWavPath,
  recordingTitleDisabled,
  importWavPathDisabled,
  chooseWavDisabled,
  startDisabled,
  importDisabled,
  stopDisabled,
  chooseWavButtonTitle,
  startButtonTitle,
  importButtonTitle,
  stopButtonTitle,
  storagePath,
  pendingCommand,
  onRecordingTitleChange,
  onImportWavPathChange,
  onChooseWav,
  onStartRecording,
  onImportWav,
  onStopRecording,
}: RecordingControlsProps) {
  return (
    <section className="recording-strip" aria-label="Recording controls and status">
      <div className="strip-primary">
        <IconFrame tone={recording.tone}>
          <Waveform size={22} weight="regular" />
        </IconFrame>
        <div>
          <div className="strip-heading-row">
            <h2>Recording</h2>
            <StatusPill tone={recording.tone} label={recording.label} />
          </div>
          <p>{recording.detail}</p>
        </div>
      </div>
      <div className="recording-actions">
        <label className="recording-title-field" htmlFor="recording-title">
          <span>Recording title</span>
          <input
            id="recording-title"
            value={recordingTitle}
            onChange={(event) => onRecordingTitleChange(event.target.value)}
            placeholder="Optional meeting title"
            disabled={recordingTitleDisabled}
          />
        </label>
        <div className="import-wav-control">
          <label className="recording-title-field" htmlFor="import-wav-path">
            <span>WAV source path</span>
            <input
              id="import-wav-path"
              value={importWavPath}
              onChange={(event) => onImportWavPathChange(event.target.value)}
              placeholder="/path/to/audio.wav"
              disabled={importWavPathDisabled}
            />
          </label>
          <button
            type="button"
            className="button"
            disabled={chooseWavDisabled}
            title={chooseWavButtonTitle}
            onClick={onChooseWav}
          >
            <FolderOpen size={16} weight="regular" />
            {pendingCommand === "choose-wav" ? "Choosing WAV" : "Choose WAV"}
          </button>
        </div>
        <div className="recording-buttons">
          <button
            type="button"
            className="button primary"
            disabled={startDisabled}
            title={startButtonTitle}
            onClick={onStartRecording}
          >
            <Microphone size={16} weight="regular" />
            {pendingCommand === "start" ? "Starting recording" : "Start recording"}
          </button>
          <button
            type="button"
            className="button"
            disabled={importDisabled}
            title={importButtonTitle}
            onClick={onImportWav}
          >
            <FileText size={16} weight="regular" />
            {pendingCommand === "import" ? "Importing WAV" : "Import WAV"}
          </button>
          <button
            type="button"
            className="button"
            disabled={stopDisabled}
            title={stopButtonTitle}
            onClick={onStopRecording}
          >
            <Waveform size={16} weight="regular" />
            {pendingCommand === "stop" ? "Stopping recording" : "Stop recording"}
          </button>
        </div>
        <span className="recording-path">{storagePath}</span>
      </div>
    </section>
  );
}
