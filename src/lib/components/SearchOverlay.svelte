<!--
  SearchOverlay (Ctrl+Shift+F) — Sprint 30.3.

  Project-wide substring search over every game script + loc file visible through
  the Vfs, streamed and paged by the backend `search_project` command
  (Windows-1252 script + UTF-8-BOM loc, encoding-aware). Results are grouped by
  file with a context line (match highlighted). Clicking a hit routes into the
  owning editor via `routeForFile` (src/lib/searchRoute.ts) — the host handles
  province/country/mode/overlay routes; files with no editor open a read-only
  preview here.
-->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { OverlaySurface } from "$lib/components/script";
  import { routeForFile, type SearchRoute } from "$lib/searchRoute";

  interface SearchHit { file: string; origin: "base" | "mod"; line: number; col: number; text: string }
  interface SearchResult { hits: SearchHit[]; total: number; offset: number; limit: number; capped: boolean }
  interface FileText { file: string; origin: string; text: string; binary: boolean }
  interface FamilyMeta { id: string; dir: string }

  let {
    open = $bindable(false),
    installPath,
    modPath,
    onroute,
  }: {
    open?: boolean;
    installPath: string;
    modPath: string | null;
    /** Editor routes (province/country/mode/overlay) handled by the host. */
    onroute: (route: SearchRoute, file: string, line: number) => void;
  } = $props();

  const LIMIT = 200;
  // Game-relative folders the "folder" scope can restrict to.
  const FOLDERS = ["common", "history", "map", "events", "decisions", "missions", "localisation", "gfx", "gui"];

  let query = $state("");
  let scope = $state<"base_mod" | "mod" | "folder">("base_mod");
  let folder = $state("common");
  let offset = $state(0);
  let result = $state<SearchResult | null>(null);
  let searching = $state(false);
  let seq = 0;

  // dir → mechanics family id, so common/<mechanics-dir> hits route to the
  // Mechanics overlay focused on the right family.
  let mechanicsDirs = $state<Map<string, string>>(new Map());
  $effect(() => {
    void installPath;
    void modPath;
    if (!open) return;
    invoke<FamilyMeta[]>("get_mechanic_families", { installPath, modPath })
      .then((fams) => {
        mechanicsDirs = new Map(fams.filter((f) => f.dir).map((f) => [f.dir, f.id]));
      })
      .catch(() => {});
  });

  async function runSearch() {
    if (!query.trim()) {
      result = null;
      return;
    }
    const s = ++seq;
    searching = true;
    try {
      const r = await invoke<SearchResult>("search_project", {
        installPath,
        modPath,
        query,
        scope,
        folder: scope === "folder" ? folder : null,
        offset,
        limit: LIMIT,
      });
      if (s !== seq) return;
      result = r;
    } catch {
      if (s === seq) result = null;
    } finally {
      if (s === seq) searching = false;
    }
  }

  // Debounced search on query/scope/folder change (reset paging).
  let debounce: ReturnType<typeof setTimeout> | null = null;
  $effect(() => {
    void query;
    void scope;
    void folder;
    if (!open) return;
    offset = 0;
    if (debounce) clearTimeout(debounce);
    debounce = setTimeout(() => void runSearch(), 200);
  });
  $effect(() => {
    void offset;
    if (open) void runSearch();
  });

  // Hits grouped by file, preserving backend order.
  const groups = $derived.by(() => {
    const out: { file: string; origin: string; hits: SearchHit[] }[] = [];
    const idx = new Map<string, number>();
    for (const h of result?.hits ?? []) {
      let i = idx.get(h.file);
      if (i === undefined) {
        i = out.length;
        idx.set(h.file, i);
        out.push({ file: h.file, origin: h.origin, hits: [] });
      }
      out[i].hits.push(h);
    }
    return out;
  });

  function clickHit(h: SearchHit) {
    const route = routeForFile(h.file, mechanicsDirs);
    if (route.kind === "preview") {
      void openPreview(h.file, h.line);
    } else {
      onroute(route, h.file, h.line);
      open = false;
    }
  }

  // Split a line around the (case-insensitive) match for highlight rendering.
  function parts(text: string, col: number): [string, string, string] {
    const len = query.trim().length;
    if (col < 0 || len === 0) return [text, "", ""];
    return [text.slice(0, col), text.slice(col, col + len), text.slice(col + len)];
  }

  // --- Read-only preview (files with no editor) -----------------------------
  let preview = $state<{ file: string; line: number; text: string; binary: boolean } | null>(null);
  async function openPreview(file: string, line: number) {
    try {
      const ft = await invoke<FileText>("read_project_file", { installPath, modPath, rel: file });
      preview = { file, line, text: ft.text, binary: ft.binary };
    } catch (e) {
      preview = { file, line, text: `Failed to read file: ${e}`, binary: false };
    }
  }
  const previewLines = $derived(preview ? preview.text.split(/\r?\n/) : []);

  // Focus the query field when the toolbar mounts (i.e. when the overlay opens).
  function focusOnMount(node: HTMLInputElement) {
    node.focus();
  }
</script>

<OverlaySurface bind:open title="Find in Project">
  {#snippet toolbar()}
    <input class="q" placeholder="Search all files…" bind:value={query} use:focusOnMount />
    <select class="scope" bind:value={scope} title="Search scope">
      <option value="base_mod">Base + Mod</option>
      <option value="mod">Mod only</option>
      <option value="folder">Folder…</option>
    </select>
    {#if scope === "folder"}
      <select class="scope" bind:value={folder} title="Game folder">
        {#each FOLDERS as d}<option value={d}>{d}/</option>{/each}
      </select>
    {/if}
    {#if searching}<span class="spin">…</span>{/if}
    {#if result}
      <span class="stats">
        {#if result.total > 0}
          {result.total}{result.capped ? "+" : ""} match{result.total === 1 ? "" : "es"} in {groups.length} file{groups.length === 1 ? "" : "s"}
        {:else}No matches{/if}
      </span>
    {/if}
  {/snippet}

  <div class="results">
    {#if !query.trim()}
      <p class="hint">Type to search every script and localisation file in the project.</p>
    {:else if result && result.total === 0 && !searching}
      <p class="hint">No matches for “{query}”.</p>
    {:else}
      {#each groups as g (g.file)}
        <section class="file-group">
          <header class="file-head">
            <span class="fname" title={g.file}>{g.file}</span>
            <span class="origin" class:mod={g.origin === "mod"}>{g.origin}</span>
            <span class="fcount">{g.hits.length}</span>
          </header>
          <ul class="hits">
            {#each g.hits as h}
              {@const p = parts(h.text, h.col)}
              <li>
                <button class="hit" onclick={() => clickHit(h)} title="Open">
                  <span class="ln">{h.line}</span>
                  <span class="ctx"><span>{p[0]}</span><mark>{p[1]}</mark><span>{p[2]}</span></span>
                </button>
              </li>
            {/each}
          </ul>
        </section>
      {/each}
      {#if result && result.total > offset + LIMIT}
        <div class="pager">
          <button disabled={offset === 0} onclick={() => (offset = Math.max(0, offset - LIMIT))}>◀ Prev</button>
          <span>{result.total - (offset + LIMIT)} more…</span>
          <button onclick={() => (offset += LIMIT)}>Next ▶</button>
        </div>
      {/if}
    {/if}
  </div>
</OverlaySurface>

{#if preview}
  <div class="pv-root">
    <button class="pv-backdrop" aria-label="Close preview" onclick={() => (preview = null)}></button>
    <div class="pv-panel" role="dialog" aria-modal="true" aria-label={preview.file}>
      <header class="pv-head">
        <span class="pv-title" title={preview.file}>{preview.file}</span>
        <span class="pv-note">read-only</span>
        <button class="pv-close" onclick={() => (preview = null)}>×</button>
      </header>
      <div class="pv-body">
        {#if preview.binary}
          <p class="hint">Binary file — no text preview.</p>
        {:else}
          <pre class="pv-pre">{#each previewLines as ln, i}<div class="pv-ln" class:match={i + 1 === preview.line}><span class="pv-lno">{i + 1}</span>{ln}
</div>{/each}</pre>
        {/if}
      </div>
    </div>
  </div>
{/if}

<style>
  .q { flex: 1; min-width: 12rem; background: #16191f; border: 1px solid #1f242c; color: #cfd4db; padding: 0.3rem 0.5rem; font-family: inherit; font-size: 0.85rem; }
  .scope { background: #16191f; border: 1px solid #1f242c; color: #cfd4db; font-family: inherit; font-size: 0.78rem; padding: 0.25rem; }
  .spin { color: #8a919c; }
  .stats { font-size: 0.74rem; color: #8a919c; white-space: nowrap; }

  .results { display: flex; flex-direction: column; gap: 0.5rem; }
  .hint { color: #8a919c; font-size: 0.82rem; padding: 0.6rem 0.2rem; }

  .file-group { border: 1px solid #1f242c; background: #262c35; }
  .file-head { display: flex; align-items: center; gap: 0.5rem; padding: 0.28rem 0.5rem; background: #21262e; border-bottom: 1px solid #1f242c; }
  .fname { flex: 1; min-width: 0; color: #9aecc0; font-size: 0.8rem; font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .origin { font-size: 0.64rem; border: 1px solid #3a434f; padding: 0 0.25rem; color: #9aa2ad; }
  .origin.mod { color: #9aecc0; border-color: #2f5f48; }
  .fcount { font-size: 0.7rem; color: #8a919c; font-variant-numeric: tabular-nums; }

  .hits { list-style: none; margin: 0; padding: 0; }
  .hit { display: flex; align-items: baseline; gap: 0.55rem; width: 100%; text-align: left; border: none; border-bottom: 1px solid #21262e; background: transparent; color: #cfd4db; font: inherit; font-size: 0.78rem; padding: 0.16rem 0.55rem; cursor: pointer; }
  .hit:hover { background: #4a6da7; color: #fff; }
  .ln { flex: 0 0 auto; width: 3rem; text-align: right; color: #6b7482; font-variant-numeric: tabular-nums; }
  .hit:hover .ln { color: #cdd; }
  .ctx { flex: 1; min-width: 0; font-family: "Cascadia Code", "Consolas", monospace; white-space: pre; overflow: hidden; text-overflow: ellipsis; }
  .ctx mark { background: #d8a020; color: #1a1d22; }

  .pager { display: flex; align-items: center; gap: 0.6rem; padding: 0.5rem 0.2rem; color: #8a919c; font-size: 0.76rem; }
  .pager button { border: 1px solid #3a434f; background: #2b323d; color: #cfd4db; font-family: inherit; font-size: 0.72rem; padding: 0.15rem 0.6rem; cursor: pointer; }
  .pager button:disabled { opacity: 0.4; cursor: default; }

  /* Read-only preview modal (z above the overlay panel's 101). */
  .pv-root { position: fixed; inset: 0; z-index: 110; display: flex; align-items: center; justify-content: center; }
  .pv-backdrop { position: absolute; inset: 0; border: none; background: rgba(0,0,0,0.55); cursor: default; }
  .pv-panel { position: relative; display: flex; flex-direction: column; width: 80vw; max-width: 60rem; height: 80vh; background: #2b323d; border: 1px solid #1f242c; box-shadow: 0 10px 40px rgba(0,0,0,0.6); }
  .pv-head { display: flex; align-items: center; gap: 0.6rem; padding: 0.4rem 0.6rem; background: #3f4855; border-bottom: 1px solid #1f242c; }
  .pv-title { flex: 1; min-width: 0; font-weight: 700; font-size: 0.85rem; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .pv-note { font-size: 0.68rem; color: #d8a020; border: 1px solid #6b5720; padding: 0 0.3rem; }
  .pv-close { border: none; background: transparent; color: #cfd4db; font-size: 1.3rem; line-height: 1; cursor: pointer; }
  .pv-close:hover { color: #fff; }
  .pv-body { flex: 1; min-height: 0; overflow: auto; background: #16191f; }
  .pv-pre { margin: 0; font-family: "Cascadia Code", "Consolas", monospace; font-size: 0.76rem; color: #cfd4db; }
  .pv-ln { display: block; white-space: pre; padding: 0 0.5rem; }
  .pv-ln.match { background: #3a3320; }
  .pv-lno { display: inline-block; width: 3.5rem; margin-right: 0.6rem; text-align: right; color: #556; user-select: none; }
</style>
