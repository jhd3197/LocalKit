import { defineConfig } from "vitest/config";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));

// Unit tests run without a Tauri runtime. Alias the Tauri API surface to the
// same in-browser mocks the `--mode mock` build uses, so importing `ipc.ts` or
// the settings store never reaches a real IPC bridge (imports resolve; calls
// hit the mocks). jsdom gives the pure logic a DOM (KeyboardEvent, localStorage,
// document.hasFocus) without a browser.
export default defineConfig({
  resolve: {
    alias: {
      "@tauri-apps/api/core": path.resolve(__dirname, "src/mock/core.ts"),
      "@tauri-apps/api/event": path.resolve(__dirname, "src/mock/event.ts"),
      "@tauri-apps/plugin-opener": path.resolve(__dirname, "src/mock/opener.ts"),
      "@tauri-apps/plugin-notification": path.resolve(__dirname, "src/mock/notification.ts"),
    },
  },
  test: {
    environment: "jsdom",
    include: ["src/**/*.test.ts"],
  },
});
