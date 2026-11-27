import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { readFileSync } from "node:fs";

const { version } = JSON.parse(readFileSync(new URL("../tauri.conf.json", import.meta.url), "utf8"));

export default defineConfig({
  base: "./",
  define: { __PANE_VERSION__: JSON.stringify(version) },
  plugins: [react(), tailwindcss()],
  build: { outDir: "../ui", emptyOutDir: true },
});
