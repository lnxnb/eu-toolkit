import { defineConfig } from "vitest/config";
import { fileURLToPath } from "node:url";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// Standalone vitest config (does NOT load the SvelteKit plugin) — the unit tests
// here exercise pure TypeScript logic (e.g. mission board layout composition,
// adjacency geometry), so a plain Node environment keeps them fast and free of
// the app's build graph. The `$lib` alias mirrors SvelteKit so pure modules can
// import their sibling helpers (type-only imports of rune modules are erased).
export default defineConfig({
  plugins: [svelte()],
  resolve: {
    alias: {
      $lib: fileURLToPath(new URL("./src/lib", import.meta.url)),
    },
  },
  test: {
    include: ["src/**/*.test.ts"],
    environment: "node",
  },
});
