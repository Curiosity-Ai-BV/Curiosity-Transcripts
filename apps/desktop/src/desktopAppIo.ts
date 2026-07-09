import { open } from "@tauri-apps/plugin-dialog";

export interface AppFilePicker {
  chooseImportWavPath(): Promise<string | null>;
  chooseWhisperModelPath(): Promise<string | null>;
}

export interface AppClipboardWriter {
  writeText(text: string): Promise<void>;
}

export const defaultAppFilePicker: AppFilePicker = {
  chooseImportWavPath: chooseNativeImportWavPath,
  chooseWhisperModelPath: chooseNativeWhisperModelPath,
};

export const defaultClipboardWriter: AppClipboardWriter = {
  async writeText(text: string) {
    const writeText = globalThis.navigator?.clipboard?.writeText;
    if (!writeText) {
      throw new Error("Clipboard API unavailable.");
    }
    await writeText.call(globalThis.navigator.clipboard, text);
  },
};

async function chooseNativeImportWavPath(): Promise<string | null> {
  const selected: string | string[] | null = await open({
    title: "Choose WAV audio file",
    multiple: false,
    directory: false,
    fileAccessMode: "scoped",
    filters: [
      {
        name: "WAV audio",
        extensions: ["wav"],
      },
    ],
  });

  if (Array.isArray(selected)) {
    return typeof selected[0] === "string" ? selected[0] : null;
  }

  return selected;
}

async function chooseNativeWhisperModelPath(): Promise<string | null> {
  const selected: string | string[] | null = await open({
    title: "Choose Whisper model file",
    multiple: false,
    directory: false,
    fileAccessMode: "scoped",
    filters: [
      {
        name: "Whisper model",
        extensions: ["bin", "gguf"],
      },
    ],
  });

  if (Array.isArray(selected)) {
    return typeof selected[0] === "string" ? selected[0] : null;
  }

  return selected;
}
