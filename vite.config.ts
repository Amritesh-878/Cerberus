import process from "node:process";

import solid from "vite-plugin-solid";
import { defineConfig } from "vitest/config";

const host = process.env.TAURI_DEV_HOST;

export default defineConfig(({ mode }) => ({
  // Solid's HMR runtime cannot resolve under Vitest.
  plugins: [solid({ hot: mode !== "test" })],
  // Keeps Rust compiler errors on screen during `tauri dev`.
  clearScreen: false,
  server: {
    // tauri.conf.json's devUrl expects this exact port.
    port: 1420,
    strictPort: true,
    host: host ?? false,
    hmr: host ? { protocol: "ws", host, port: 1421 } : undefined,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  test: {
    environment: "happy-dom",
  },
}));
