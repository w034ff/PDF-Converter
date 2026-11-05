import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// The page images and OcrPage JSON are shared with the Slint version.
export default defineConfig({
  plugins: [react()],
  publicDir: "../fixtures",
  base: "./",
});
