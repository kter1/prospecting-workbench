import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

const host = false;

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host,
    watch: { ignored: ["**/src-tauri/**"] }
  },
  test: {
    environment: "jsdom",
    include: ["src/**/*.test.ts", "src/**/*.test.tsx"]
  }
});
