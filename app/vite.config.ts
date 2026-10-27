/// <reference types="vitest/config" />
// Vite configuration of the OpenWraith frontend.
//
// Modes:
// - development (`npm run dev:web`, started by `tauri dev`): port 1420;
// - mock (`npm run dev:preview`): the browser preview with a mocked IPC, port 1421;
// - production (`npm run build:web`): the bundle embedded in the app.
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

/** The release CSP (same as `src-tauri/tauri.conf.json`), used by `vite preview`. */
const CSP =
  "default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data:; " +
  "font-src 'self'; connect-src ipc: http://ipc.localhost; object-src 'none'; " +
  "base-uri 'none'; form-action 'none'; frame-ancestors 'none'";

export default defineConfig(({ mode }) => ({
  plugins: [react()],
  // Keep Rust errors visible in `tauri dev`.
  clearScreen: false,
  envPrefix: ["VITE_"],
  server: {
    host: "localhost",
    port: mode === "mock" ? 1421 : 1420,
    strictPort: true,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  preview: {
    headers: { "Content-Security-Policy": CSP },
  },
  build: {
    target: "es2022",
    outDir: "dist",
    emptyOutDir: true,
    sourcemap: false,
    // No inline data URIs for scripts or styles; images may be inlined (img-src data:).
    assetsInlineLimit: 0,
    // `dist/.vite/license.md`: the licenses of the bundled packages, merged
    // into THIRD-PARTY-NOTICES.txt by `scripts/third-party-notices.mjs`.
    license: true,
  },
  test: {
    environment: "jsdom",
    setupFiles: ["src/test/setup.ts"],
    include: ["src/**/*.test.{ts,tsx}"],
    restoreMocks: true,
    // Process CSS so that `?raw` imports of stylesheets (the contrast and
    // layout tests) return their text. Only main.tsx imports these sheets.
    css: { include: [/tokens\.css/, /shell\.css/, /components\.css/] },
  },
}));
