import { FolderOpen } from "@phosphor-icons/react";

import type { PersistedRawAudioRetentionPolicy } from "./commandAdapter";
import { DesktopSettingsFeedback, type SettingsFeedback } from "./desktopSettingsFeedback";
import type { PendingCommand, SettingsFormState } from "./desktopWorkspaceState";

export interface DesktopSettingsFormProps {
  settingsForm: SettingsFormState;
  settingsFeedback: SettingsFeedback | null;
  pendingCommand: PendingCommand;
  settingsInputDisabled: boolean;
  settingsActionDisabled: boolean;
  chooseWhisperModelDisabled: boolean;
  chooseWhisperModelButtonTitle: string;
  testWhisperButtonTitle: string;
  saveWhisperButtonTitle: string;
  testOllamaButtonTitle: string;
  saveAnalysisButtonTitle: string;
  saveRetentionButtonTitle: string;
  copyPullCommandDisabled: boolean;
  onWhisperModelPathChange(value: string): void;
  onChooseWhisperModel(): void;
  onTestWhisperModelPath(): void;
  onSaveWhisperModelPath(): void;
  onOllamaBaseUrlChange(value: string): void;
  onOllamaModelChange(value: string): void;
  onTestOllamaConnection(): void;
  onSaveAnalysisSettings(): void;
  onRawAudioRetentionPolicyChange(value: PersistedRawAudioRetentionPolicy): void;
  onSaveRawAudioRetentionPolicy(): void;
  onCopyPullCommand(pullCommand: string): Promise<void>;
}

export function DesktopSettingsForm({
  settingsForm,
  settingsFeedback,
  pendingCommand,
  settingsInputDisabled,
  settingsActionDisabled,
  chooseWhisperModelDisabled,
  chooseWhisperModelButtonTitle,
  testWhisperButtonTitle,
  saveWhisperButtonTitle,
  testOllamaButtonTitle,
  saveAnalysisButtonTitle,
  saveRetentionButtonTitle,
  copyPullCommandDisabled,
  onWhisperModelPathChange,
  onChooseWhisperModel,
  onTestWhisperModelPath,
  onSaveWhisperModelPath,
  onOllamaBaseUrlChange,
  onOllamaModelChange,
  onTestOllamaConnection,
  onSaveAnalysisSettings,
  onRawAudioRetentionPolicyChange,
  onSaveRawAudioRetentionPolicy,
  onCopyPullCommand,
}: DesktopSettingsFormProps) {
  return (
    <div className="settings-form" aria-label="Local settings">
      <div className="path-picker-control">
        <label className="settings-field" htmlFor="whisper-model-path">
          <span>Whisper model path</span>
          <input
            id="whisper-model-path"
            value={settingsForm.whisperModelPath}
            onChange={(event) => onWhisperModelPathChange(event.target.value)}
            placeholder="/absolute/path/to/ggml-base.en.bin"
            disabled={settingsInputDisabled}
          />
        </label>
        <button
          type="button"
          className="button"
          disabled={chooseWhisperModelDisabled}
          title={chooseWhisperModelButtonTitle}
          onClick={onChooseWhisperModel}
        >
          <FolderOpen size={16} weight="regular" />
          {pendingCommand === "choose-whisper-model" ? "Choosing model" : "Choose model"}
        </button>
      </div>
      <div className="settings-buttons">
        <button
          type="button"
          className="button"
          disabled={settingsActionDisabled}
          title={testWhisperButtonTitle}
          onClick={onTestWhisperModelPath}
        >
          {pendingCommand === "test-whisper" ? "Testing path" : "Test path"}
        </button>
        <button
          type="button"
          className="button"
          disabled={settingsActionDisabled}
          title={saveWhisperButtonTitle}
          onClick={onSaveWhisperModelPath}
        >
          {pendingCommand === "save-whisper" ? "Saving Whisper" : "Save Whisper"}
        </button>
      </div>
      <label className="settings-field" htmlFor="ollama-base-url">
        <span>Ollama base URL</span>
        <input
          id="ollama-base-url"
          value={settingsForm.ollamaBaseUrl}
          onChange={(event) => onOllamaBaseUrlChange(event.target.value)}
          placeholder="http://127.0.0.1:11434"
          disabled={settingsInputDisabled}
        />
      </label>
      <label className="settings-field" htmlFor="ollama-model">
        <span>Ollama model</span>
        <input
          id="ollama-model"
          value={settingsForm.ollamaModel}
          onChange={(event) => onOllamaModelChange(event.target.value)}
          placeholder="qwen3.6:27b"
          disabled={settingsInputDisabled}
        />
      </label>
      <div className="settings-buttons">
        <button
          type="button"
          className="button"
          disabled={settingsActionDisabled}
          title={testOllamaButtonTitle}
          onClick={onTestOllamaConnection}
        >
          {pendingCommand === "test-ollama" ? "Testing Ollama" : "Test Ollama"}
        </button>
        <button
          type="button"
          className="button"
          disabled={settingsActionDisabled}
          title={saveAnalysisButtonTitle}
          onClick={onSaveAnalysisSettings}
        >
          {pendingCommand === "save-analysis" ? "Saving analysis" : "Save analysis"}
        </button>
      </div>
      <label className="settings-field" htmlFor="raw-audio-retention">
        <span>Raw audio retention</span>
        <select
          id="raw-audio-retention"
          value={settingsForm.rawAudioRetentionPolicy}
          onChange={(event) =>
            onRawAudioRetentionPolicyChange(event.target.value as PersistedRawAudioRetentionPolicy)
          }
          disabled={settingsInputDisabled}
        >
          <option value="Retain">Retain</option>
          <option value="DeleteAfterTranscription">Delete after transcription</option>
        </select>
      </label>
      <div className="settings-buttons">
        <button
          type="button"
          className="button"
          disabled={settingsActionDisabled}
          title={saveRetentionButtonTitle}
          onClick={onSaveRawAudioRetentionPolicy}
        >
          {pendingCommand === "save-retention" ? "Saving retention" : "Save retention"}
        </button>
      </div>
      <DesktopSettingsFeedback
        feedback={settingsFeedback}
        copyPullCommandDisabled={copyPullCommandDisabled}
        onCopyPullCommand={onCopyPullCommand}
      />
    </div>
  );
}
