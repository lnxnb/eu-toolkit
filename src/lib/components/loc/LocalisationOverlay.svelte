<!--
  LocalisationOverlay (View ▸ Localisation…) — Sprint 28.

  Two tabs:
    • Search — every loc key across the VFS (origin file + language), paged/capped
      for perf; editing a value queues a loc override.
    • Missing — project-created content (decisions/events/missions the mod defines)
      whose expected loc keys have no resolved value, each with a "create loc"
      affordance that queues the override.
-->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { OverlaySurface } from "$lib/components/script";
  import type { EditQueue } from "$lib/edits.svelte";

  interface LocHit { key: string; value: string; file: string; origin: "base" | "mod"; language: string }
  interface LocSearchResult { hits: LocHit[]; total: number; offset: number; limit: number }
  interface MissingLoc { key: string; kind: string; entity: string; file: string }

  let {
    open = $bindable(false),
    installPath,
    modPath,
    queue,
  }: {
    open?: boolean;
    installPath: string;
    modPath: string | null;
    queue: EditQueue;
  } = $props();

  let tab = $state<"search" | "missing">("search");

  // --- Search ---------------------------------------------------------------
  const LIMIT = 100;
  let query = $state("");
  let offset = $state(0);
  let result = $state<LocSearchResult | null>(null);
  let searching = $state(false);
  let seq = 0;
  // Pending value overrides typed this session (key -> value), for optimistic UI.
  let overrides = $state<Map<string, string>>(new Map());

  async function runSearch() {
    const s = ++seq;
    searching = true;
    try {
      const r = await invoke<LocSearchResult>("search_loc", {
        installPath,
        modPath,
        query,
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

  // Debounced search on query change (reset paging).
  let debounce: ReturnType<typeof setTimeout> | null = null;
  $effect(() => {
    void query;
    if (!open) return;
    offset = 0;
    if (debounce) clearTimeout(debounce);
    debounce = setTimeout(() => void runSearch(), 200);
  });

  $effect(() => {
    void offset;
    void open;
    void installPath;
    void modPath;
    if (open) void runSearch();
  });

  function editValue(hit: LocHit, value: string) {
    if (value === (overrides.get(hit.key) ?? hit.value)) return;
    overrides.set(hit.key, value);
    overrides = new Map(overrides);
    queue.push({ label: `Loc ${hit.key}`, edits: [{ kind: "locOverride", key: hit.key, value }] });
  }

  const pageStart = $derived((result?.offset ?? 0) + 1);
  const pageEnd = $derived(Math.min((result?.offset ?? 0) + (result?.hits.length ?? 0), result?.total ?? 0));

  // --- Missing --------------------------------------------------------------
  let missing = $state<MissingLoc[]>([]);
  let missingLoaded = $state(false);
  let missingDraft = $state<Map<string, string>>(new Map());

  async function loadMissing() {
    try {
      missing = await invoke<MissingLoc[]>("missing_loc_report", { installPath, modPath });
    } catch {
      missing = [];
    }
    missingLoaded = true;
  }

  $effect(() => {
    void open;
    void tab;
    void installPath;
    void modPath;
    queue.version;
    if (open && tab === "missing") void loadMissing();
  });

  function createLoc(m: MissingLoc) {
    const value = (missingDraft.get(m.key) ?? "").trim();
    if (!value) return;
    queue.push({ label: `Create loc ${m.key}`, edits: [{ kind: "locOverride", key: m.key, value }] });
    // Optimistically drop it from the missing list.
    missing = missing.filter((x) => x.key !== m.key);
  }
</script>

<OverlaySurface bind:open title="Localisation">
  <div class="tabs">
    <button class="tab" class:on={tab === "search"} onclick={() => (tab = "search")}>Search</button>
    <button class="tab" class:on={tab === "missing"} onclick={() => (tab = "missing")}>
      Missing{#if missingLoaded && missing.length} <span class="badge">{missing.length}</span>{/if}
    </button>
  </div>

  {#if tab === "search"}
    <div class="search-bar">
      <input class="q" placeholder="Search keys and values…" bind:value={query} />
      {#if searching}<span class="spin">…</span>{/if}
      {#if result}
        <span class="stats">
          {#if result.total > 0}{pageStart}–{pageEnd} of {result.total}{:else}No matches{/if}
        </span>
        <div class="pager">
          <button disabled={offset === 0} onclick={() => (offset = Math.max(0, offset - LIMIT))}>◀ Prev</button>
          <button disabled={offset + LIMIT >= result.total} onclick={() => (offset += LIMIT)}>Next ▶</button>
        </div>
      {/if}
    </div>
    <table class="rows">
      <thead>
        <tr><th class="k">Key</th><th class="v">Value</th><th class="o">Origin</th><th class="f">File</th><th class="l">Lang</th></tr>
      </thead>
      <tbody>
        {#each result?.hits ?? [] as hit (hit.file + hit.key)}
          <tr>
            <td class="k"><code>{hit.key}</code></td>
            <td class="v">
              <input value={overrides.get(hit.key) ?? hit.value} onchange={(e) => editValue(hit, e.currentTarget.value)} />
            </td>
            <td class="o"><span class="origin" class:mod={hit.origin === "mod"}>{hit.origin}</span></td>
            <td class="f" title={hit.file}>{hit.file}</td>
            <td class="l">{hit.language}</td>
          </tr>
        {/each}
      </tbody>
    </table>
    {#if result && result.total > offset + LIMIT}
      <p class="more">{result.total - (offset + LIMIT)} more… use Next ▶ to page.</p>
    {/if}
  {:else}
    <div class="missing">
      <p class="explain">
        Loc keys that project-created content (decisions / events / missions the mod
        defines) expects but that resolve nowhere. Fill a value to create the override.
      </p>
      {#if !missingLoaded}
        <p class="dim">Scanning…</p>
      {:else if missing.length === 0}
        <p class="dim">No missing localisation. 🎉</p>
      {:else}
        <table class="rows">
          <thead><tr><th>Key</th><th>Kind</th><th>Entity</th><th>New value</th><th></th></tr></thead>
          <tbody>
            {#each missing as m (m.key)}
              <tr>
                <td class="k"><code>{m.key}</code></td>
                <td><span class="kind">{m.kind}</span></td>
                <td class="ent" title={m.file}>{m.entity}</td>
                <td class="v">
                  <input
                    placeholder="Enter text…"
                    value={missingDraft.get(m.key) ?? ""}
                    oninput={(e) => { missingDraft.set(m.key, e.currentTarget.value); missingDraft = new Map(missingDraft); }}
                  />
                </td>
                <td><button class="mini" onclick={() => createLoc(m)} disabled={!(missingDraft.get(m.key) ?? "").trim()}>create</button></td>
              </tr>
            {/each}
          </tbody>
        </table>
      {/if}
    </div>
  {/if}
</OverlaySurface>

<style>
  .tabs { display: flex; gap: 0.2rem; margin-bottom: 0.5rem; }
  .tab { border: 1px solid #3a434f; background: #2b323d; color: #cfd4db; font-family: inherit; font-size: 0.8rem; padding: 0.25rem 0.8rem; cursor: pointer; }
  .tab.on { background: #4a6da7; border-color: #4a6da7; color: #fff; }
  .badge { background: #7a2e2e; color: #f2dede; padding: 0 0.3rem; font-size: 0.7rem; }
  .search-bar { display: flex; align-items: center; gap: 0.6rem; margin-bottom: 0.4rem; }
  .q { flex: 1; background: #16191f; border: 1px solid #1f242c; color: #cfd4db; padding: 0.3rem 0.5rem; font-family: inherit; font-size: 0.85rem; }
  .spin { color: #8a919c; }
  .stats { font-size: 0.74rem; color: #8a919c; white-space: nowrap; }
  .pager { display: flex; gap: 0.2rem; }
  .pager button { border: 1px solid #3a434f; background: #2b323d; color: #cfd4db; font-family: inherit; font-size: 0.72rem; padding: 0.15rem 0.5rem; cursor: pointer; }
  .pager button:disabled { opacity: 0.4; cursor: default; }
  .rows { width: 100%; border-collapse: collapse; font-size: 0.78rem; }
  .rows th { text-align: left; color: #8a919c; font-weight: 600; padding: 0.2rem 0.4rem; border-bottom: 1px solid #2b323d; position: sticky; top: 0; background: #2b323d; }
  .rows td { padding: 0.12rem 0.4rem; border-bottom: 1px solid #21262e; vertical-align: middle; }
  .rows td.k code { color: #9aecc0; background: #16191f; padding: 0 0.3rem; }
  .rows td.v input { width: 100%; background: #16191f; border: 1px solid #1f242c; color: #cfd4db; padding: 0.15rem 0.35rem; font-family: inherit; font-size: 0.78rem; }
  td.f { color: #8a919c; max-width: 16rem; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .origin { font-size: 0.66rem; border: 1px solid #3a434f; padding: 0 0.25rem; color: #9aa2ad; }
  .origin.mod { color: #9aecc0; border-color: #2f5f48; }
  .kind { font-size: 0.68rem; background: #2f4a6b; color: #bcd; padding: 0 0.3rem; }
  .more, .explain, .dim { color: #8a919c; font-size: 0.76rem; }
  .explain { margin: 0 0 0.5rem; }
  .mini { border: 1px solid #3a434f; background: #2b323d; color: #cfd4db; font-family: inherit; font-size: 0.72rem; padding: 0.1rem 0.5rem; cursor: pointer; }
  .mini:hover:not(:disabled) { background: #4a6da7; color: #fff; }
  .mini:disabled { opacity: 0.5; cursor: default; }
  td.ent { color: #cfd4db; }
</style>
