import { defineConfig } from "vite";
import { sveltekit } from "@sveltejs/kit/vite";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;

// Stable mode (run.bat): freeze the dev server — no HMR pushes, no file
// watching — so source edits made while the app is open never reload or break
// the running instance. Changes are picked up on the next run.bat launch.
// @ts-expect-error process is a nodejs global
const noWatch = !!process.env.EU_TOOLKIT_NO_WATCH;

// https://vite.dev/config/
export default defineConfig(async () => ({
  plugins: [sveltekit()],

  // Pre-bundle every bare dependency the app imports at server START so the dep
  // optimizer never re-optimizes MID-SESSION. A mid-session re-optimize forces a
  // full-page reload ("optimized dependencies changed. reloading"), which wipes
  // the in-memory session $state in +page.svelte back to the launch screen — and
  // in run.bat stable mode (hmr:false/watch:null) the reload can't even be
  // delivered, leaving a stale optimize-dep graph (504s / "nothing happens").
  // This list must contain EVERY bare (node_modules) import in src/ except
  // svelte/@sveltejs, which the svelte plugin excludes for us. Grep src/ for
  // `from "@…"` when adding new dependencies and keep this in sync.
  optimizeDeps: {
    include: [
      "@tauri-apps/api/core",
      "@tauri-apps/api/event",
      "@tauri-apps/plugin-dialog",
    ],
  },

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: noWatch
      ? false
      : host
        ? {
            protocol: "ws",
            host,
            port: 1421,
          }
        : undefined,
    watch: noWatch
      ? null // disable the file watcher entirely (stable mode)
      : {
          // 3. tell Vite to ignore watching `src-tauri`
          ignored: ["**/src-tauri/**"],
        },
  },
}));
