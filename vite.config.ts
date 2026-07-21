import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));

export default defineConfig(({ mode }) => ({
  plugins: [react()],
  resolve: {
    alias:
      // Mock/screenshot build (`vite --mode mock`): swap the Tauri API surface
      // for in-browser mocks in src/mock/ so the UI renders with fictional
      // data and no Rust backend. Normal builds are unaffected.
      mode === "mock"
        ? {
            "@tauri-apps/api/core": path.resolve(__dirname, "src/mock/core.ts"),
            "@tauri-apps/api/event": path.resolve(__dirname, "src/mock/event.ts"),
            "@tauri-apps/plugin-opener": path.resolve(__dirname, "src/mock/opener.ts"),
            "@tauri-apps/plugin-notification": path.resolve(
              __dirname,
              "src/mock/notification.ts"
            ),
          }
        : {},
  },
  // Tauri expects a fixed port in dev.
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
}));
