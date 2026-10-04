/// <reference types="vitest/config" />
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// https://vite.dev/config/
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
    watch: {
      // The Cargo workspace puts its build output in the root `target/`. On Windows,
      // watching files that rustc holds open fails with EBUSY and stops the dev server.
      ignored: ["**/src-tauri/**", "**/crates/**", "**/target/**"],
    },
  },
  test: {
    globals: true,
    environment: "jsdom",
    setupFiles: "./src/test/setup.ts",
  },
});
