/**
 * Vite configuration for the Kindle desktop frontend.
 *
 * The port and clear-screen settings are aligned with Tauri's desktop workflow
 * so `tauri dev` can attach to a predictable frontend server.
 */

import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

const minifyMode: false | "esbuild" = process.env.TAURI_DEBUG ? false : "esbuild";

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  envPrefix: ["VITE_", "TAURI_"],
  server: {
    port: 1420,
    strictPort: true,
  },
  preview: {
    port: 1420,
    strictPort: true,
  },
  build: {
    target: process.env.TAURI_ENV_PLATFORM === "windows" ? "chrome105" : "safari13",
    minify: minifyMode,
    sourcemap: Boolean(process.env.TAURI_DEBUG),
  },
});
