import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

// Tauri expects a fixed dev port and must not watch the Rust sources.
export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  // Bundle is loaded from disk by the desktop app, so size warnings don't apply.
  build: { chunkSizeWarningLimit: 1500 },
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ["**/src-tauri/**", "**/extension/**"] },
  },
});
