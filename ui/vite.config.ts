/// <reference types="vitest/config" />
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

// Tauri serves the built files from `ui/dist`; the dev server must match `devUrl` in tauri.conf.json.
export default defineConfig({
  base: "./",
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  server: {
    host: "127.0.0.1",
    port: 1420,
    strictPort: true,
  },
  build: {
    outDir: "dist",
    target: "chrome120",
    sourcemap: false,
    chunkSizeWarningLimit: 700,
  },
  test: {
    include: ["src/**/*.test.ts"],
    environment: "node",
  },
});
