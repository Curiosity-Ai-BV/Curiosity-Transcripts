import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
  envPrefix: ["VITE_", "TAURI_"],
  test: {
    environment: "jsdom",
    setupFiles: "./vitest.setup.ts",
    coverage: {
      provider: "v8",
      reportsDirectory: "../../release-artifacts/coverage/frontend",
      reporter: ["lcovonly"],
      include: [
        "src/App.tsx",
        "src/commandAdapter.ts",
        "src/desktopContract.ts",
        "src/desktopTopbar.tsx",
        "src/desktopRecordingControls.tsx",
        "src/desktopMeetingDetailHeader.tsx",
        "src/desktopMeetingPrivacyRow.tsx",
        "src/desktopMeetingSummarySection.tsx",
        "src/desktopMeetingDetailActions.tsx",
        "src/desktopMeetingTranscriptSection.tsx",
        "src/desktopCommandOutcomes.tsx",
        "src/desktopSettingsEngineStack.tsx",
        "src/desktopModelReadiness.tsx",
        "src/desktopModelSetupOptions.tsx",
        "src/desktopCalendarContext.tsx",
        "src/desktopSettingsFeedback.tsx",
        "src/desktopSettingsForm.tsx",
      ],
    },
  },
});
