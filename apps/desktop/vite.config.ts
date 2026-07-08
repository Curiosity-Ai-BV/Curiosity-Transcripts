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
      include: ["src/App.tsx", "src/commandAdapter.ts"],
    },
  },
});
