import { defineConfig } from "vite";
import { sveltekit } from "@sveltejs/kit/vite";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;

// Stable mode (run.bat): freeze the dev server — no HMR pushes, no file
// watching — so source edits made while the app is open never reload or break
// the running instance. Changes are picked up on the next run.bat launch.
// @ts-expect-error process is a nodejs global
const noWatch = !!process.env.EU_TOOLKIT_NO_WATCH;

// UI-debug mode (run-uidebug.bat): run the dev server on its own port so an
// agent-driven instance never collides with another Tauri project's vite on
// 1420 (the toolkit webview would silently load THAT app instead). Must match
// build.devUrl in src-tauri/tauri.uidebug.conf.json.
// @ts-expect-error process is a nodejs global
const port = Number(process.env.EU_TOOLKIT_UI_PORT) || 1420;

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
      "@tauri-apps/api/window",
      "@tauri-apps/plugin-dialog",
    ],
  },

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port,
    strictPort: true,
    host: host || false,
    // Start transforming the heavy MapView module graph (~230 files) as soon
    // as the dev server boots, instead of on the webview's first request for
    // it. This overlaps the compile with the Rust build/launch phase, so the
    // dynamically-imported editor chunk (+page.svelte) is warm by the time a
    // project is opened.
    warmup: {
      clientFiles: ["./src/lib/components/MapView.svelte"],
    },
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
