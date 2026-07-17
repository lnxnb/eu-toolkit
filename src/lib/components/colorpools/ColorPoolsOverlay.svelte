<!--
  ColorPoolsOverlay — View ▸ Color Pools… (Sprint 27).

  A bespoke editor for the two NON-KEYED color-pool registries that don't fit the
  Mechanics keyed-entry Family model:
    • common/custom_country_colors — nation-designer country colors + custom-flag
      `flag_color`s + a `num_symbols` scalar + a preserved `textures` block.
    • common/dynasty_colors        — repeated `color = { r g b }` only.

  Per file: a swatch grid (each swatch reuses the shared ColorPicker widget),
  add/remove, an origin badge (base/mod), and a note for any preserved unknown
  blocks. Editing an existing base file copies it into the project (copy-on-write,
  every original color preserved) and shadows base; a fresh `zz_eutoolkit_*` file
  ADDS its colors to the base pool.

  Edits are byte-surgical. Existing files diff to occurrence-indexed setBlock /
  removeStatement / insertStatement / setScalar edits (colors changed in place,
  removals emitted high-occurrence-first so lower indices stay valid, adds
  appended at EOF). A brand-new file is rewritten wholesale via createFile. Each
  file owns exactly one queue composite (tagged `colorpool:<rel>`), replaced on
  every change, so occurrence indices never drift across a session.
-->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { OverlaySurface } from "$lib/components/script";
  import { ColorPicker, type RGB } from "$lib/components/ui";
  import type { EditQueue, TypedEdit } from "$lib/edits.svelte";

  let {
    open = $bindable(false),
    installPath,
    modPath = null,
    queue,
  }: {
    open?: boolean;
    installPath: string;
    modPath?: string | null;
    queue: EditQueue;
  } = $props();

  // --- Backend wire types (mirror color_pools.rs) ---
  interface WireColor {
    key: "color" | "flag_color";
    occ: number;
    r: number;
    g: number;
    b: number;
    comment: string | null;
  }
  interface WireScalar {
    key: string;
    value: string;
  }
  interface WireFile {
    pool: string;
    rel: string;
    origin: string;
    colors: WireColor[];
    scalars: WireScalar[];
    extra_keys: string[];
  }
  interface WireGroup {
    pool: string;
    dir: string;
    new_file: string;
    files: WireFile[];
  }
  interface WirePools {
    groups: WireGroup[];
  }

  // --- Editable working model ---
  interface WorkColor {
    key: "color" | "flag_color";
    r: number;
    g: number;
    b: number;
    comment: string | null;
    /** Original occurrence within its key group, or null when newly added. */
    src: number | null;
    uid: number;
  }
  interface WorkFile {
    pool: string;
    rel: string;
    origin: string;
    isNew: boolean;
    colors: WorkColor[];
    scalars: WorkScalar[];
    extraKeys: string[];
    origByKey: Record<string, RGB[]>;
    origScalars: Record<string, string>;
  }
  interface WorkScalar {
    key: string;
    value: string;
  }
  interface WorkGroup {
    pool: string;
    dir: string;
    newFile: string;
    label: string;
    files: WorkFile[];
  }

  const POOL_LABEL: Record<string, string> = {
    custom_country_colors: "Custom Country Colors",
    dynasty_colors: "Dynasty Colors",
  };

  let groups = $state<WorkGroup[]>([]);
  let selectedPool = $state<string>("custom_country_colors");
  let loading = $state(false);
  let loaded = $state(false);
  let error = $state<string | null>(null);
  let uidSeq = 0;

  const selectedGroup = $derived(groups.find((g) => g.pool === selectedPool) ?? null);

  // Lazy load on first open; keep the working model across open/close so pending
  // edits stay visible (a session switch remounts and reloads).
  $effect(() => {
    if (open && !loaded && !loading && installPath) {
      void load();
    }
  });

  async function load() {
    loading = true;
    error = null;
    try {
      const wire = await invoke<WirePools>("get_color_pools", {
        installPath,
        modPath,
      });
      groups = wire.groups.map(toWorkGroup);
      if (!groups.some((g) => g.pool === selectedPool) && groups.length) {
        selectedPool = groups[0].pool;
      }
      loaded = true;
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  function toWorkGroup(g: WireGroup): WorkGroup {
    return {
      pool: g.pool,
      dir: g.dir,
      newFile: g.new_file,
      label: POOL_LABEL[g.pool] ?? g.pool,
      files: g.files.map((f) => toWorkFile(f)),
    };
  }

  function toWorkFile(f: WireFile): WorkFile {
    const origByKey: Record<string, RGB[]> = {};
    for (const c of f.colors) {
      (origByKey[c.key] ??= [])[c.occ] = { r: c.r, g: c.g, b: c.b };
    }
    const origScalars: Record<string, string> = {};
    for (const s of f.scalars) origScalars[s.key] = s.value;
    return {
      pool: f.pool,
      rel: f.rel,
      origin: f.origin,
      isNew: false,
      colors: f.colors.map((c) => ({
        key: c.key,
        r: c.r,
        g: c.g,
        b: c.b,
        comment: c.comment,
        src: c.occ,
        uid: uidSeq++,
      })),
      scalars: f.scalars.map((s) => ({ key: s.key, value: s.value })),
      extraKeys: f.extra_keys,
      origByKey,
      origScalars,
    };
  }

  function occPath(key: string, occ: number): string {
    return occ === 0 ? key : `${key}#${occ}`;
  }

  function serializeNew(f: WorkFile): string {
    let out = "";
    for (const s of f.scalars) out += `${s.key} = ${s.value}\n`;
    for (const key of ["color", "flag_color"]) {
      for (const c of f.colors) {
        if (c.key === key) out += `${c.key} = { ${c.r} ${c.g} ${c.b} }\n`;
      }
    }
    return out;
  }

  // Rebuild this file's single queue composite from its current working state.
  function sync(f: WorkFile) {
    const edits: TypedEdit[] = [];
    if (f.isNew) {
      edits.push({ kind: "createFile", file: f.rel, text: serializeNew(f) });
    } else {
      // Scalars (num_symbols): setScalar when the value changed.
      for (const s of f.scalars) {
        if (f.origScalars[s.key] !== s.value) {
          edits.push({ kind: "setScalar", file: f.rel, path: [s.key], value: s.value, quoted: false });
        }
      }
      // Changed kept colors → setBlock on the original occurrence (applied first,
      // while the file structure still matches the original).
      for (const c of f.colors) {
        if (c.src === null) continue;
        const o = f.origByKey[c.key]?.[c.src];
        if (o && (o.r !== c.r || o.g !== c.g || o.b !== c.b)) {
          edits.push({ kind: "setBlock", file: f.rel, path: [occPath(c.key, c.src)], value: `${c.r} ${c.g} ${c.b}` });
        }
      }
      // Removals: original occurrences no longer present, high-index first so
      // lower occurrences stay valid as each removal shifts the tail.
      const present = new Set(
        f.colors.filter((c) => c.src !== null).map((c) => `${c.key}#${c.src}`),
      );
      const removals: { key: string; occ: number }[] = [];
      for (const [key, arr] of Object.entries(f.origByKey)) {
        for (let i = 0; i < arr.length; i++) {
          if (arr[i] && !present.has(`${key}#${i}`)) removals.push({ key, occ: i });
        }
      }
      removals.sort((a, b) => b.occ - a.occ);
      for (const rem of removals) {
        edits.push({ kind: "removeStatement", file: f.rel, blockPath: [], key: occPath(rem.key, rem.occ) });
      }
      // Additions appended at EOF (don't shift existing occurrences).
      for (const c of f.colors) {
        if (c.src === null) {
          edits.push({ kind: "insertStatement", file: f.rel, blockPath: [], statement: `${c.key} = { ${c.r} ${c.g} ${c.b} }` });
        }
      }
    }
    const tag = `colorpool:${f.rel}`;
    queue.removeWhere((c) => c.coalesceKey === tag);
    if (edits.length) {
      queue.push({ label: `Edit ${POOL_LABEL[f.pool] ?? f.pool}`, edits, coalesceKey: tag });
    }
  }

  function setColor(f: WorkFile, c: WorkColor, rgb: RGB) {
    c.r = rgb.r;
    c.g = rgb.g;
    c.b = rgb.b;
    sync(f);
  }

  function addColor(f: WorkFile, key: "color" | "flag_color") {
    // Insert after the last color of the same key so the grid stays grouped.
    let idx = -1;
    for (let i = f.colors.length - 1; i >= 0; i--) {
      if (f.colors[i].key === key) {
        idx = i;
        break;
      }
    }
    const entry: WorkColor = { key, r: 128, g: 128, b: 128, comment: null, src: null, uid: uidSeq++ };
    if (idx < 0) f.colors.push(entry);
    else f.colors.splice(idx + 1, 0, entry);
    sync(f);
  }

  function removeColor(f: WorkFile, c: WorkColor) {
    f.colors = f.colors.filter((x) => x.uid !== c.uid);
    sync(f);
  }

  function setScalar(f: WorkFile, s: WorkScalar, value: string) {
    s.value = value;
    sync(f);
  }

  function createNewFile(g: WorkGroup) {
    if (g.files.some((f) => f.rel === g.newFile)) {
      selectedPool = g.pool;
      return;
    }
    const seed: WorkColor[] = [
      { key: "color", r: 128, g: 128, b: 128, comment: null, src: null, uid: uidSeq++ },
    ];
    const f: WorkFile = {
      pool: g.pool,
      rel: g.newFile,
      origin: "mod",
      isNew: true,
      colors: seed,
      scalars: [],
      extraKeys: [],
      origByKey: {},
      origScalars: {},
    };
    g.files = [...g.files, f];
    sync(f);
  }

  function colorsOf(f: WorkFile, key: "color" | "flag_color"): WorkColor[] {
    return f.colors.filter((c) => c.key === key);
  }
</script>

<OverlaySurface bind:open title="Color Pools">
  {#snippet toolbar()}
    <div class="pool-tabs">
      {#each groups as g (g.pool)}
        <button class:active={g.pool === selectedPool} onclick={() => (selectedPool = g.pool)}>
          {g.label}
        </button>
      {/each}
    </div>
  {/snippet}

  <div class="body">
    {#if loading}
      <p class="status">Loading…</p>
    {:else if error}
      <p class="status err">{error}</p>
    {:else if selectedGroup}
      <p class="pool-note">
        {#if selectedGroup.pool === "custom_country_colors"}
          Nation-designer country colors, custom-flag colors, and the flag-symbol
          count. Editing a base file copies it into your project (all colors kept)
          and shadows base.
        {:else}
          Colors the game assigns to dynasties. Editing a base file copies it into
          your project (all colors kept) and shadows base.
        {/if}
      </p>

      {#if selectedGroup.files.length === 0}
        <p class="status">No pool files found in {selectedGroup.dir}.</p>
      {/if}

      {#each selectedGroup.files as f (f.rel)}
        <section class="file">
          <header>
            <span class="rel">{f.rel}</span>
            <span class="badge {f.origin}">{f.origin}</span>
            {#if f.isNew}<span class="badge new">new · adds to pool</span>{/if}
          </header>

          {#if f.scalars.length}
            <div class="scalars">
              {#each f.scalars as s (s.key)}
                <label class="scalar">
                  <span>{s.key}</span>
                  <input
                    type="number"
                    min="0"
                    value={s.value}
                    oninput={(e) => setScalar(f, s, e.currentTarget.value)}
                  />
                </label>
              {/each}
            </div>
          {/if}

          {#each ["color", "flag_color"] as key (key)}
            {@const list = colorsOf(f, key as "color" | "flag_color")}
            {#if list.length || key === "color"}
              <div class="section-head">
                <h4>{key === "color" ? "Colors" : "Flag colors"} ({list.length})</h4>
                <button class="add" onclick={() => addColor(f, key as "color" | "flag_color")}>
                  ＋ Add
                </button>
              </div>
              <div class="grid">
                {#each list as c (c.uid)}
                  <div class="cell" title={c.comment ?? `${c.r} ${c.g} ${c.b}`}>
                    <ColorPicker
                      value={{ r: c.r, g: c.g, b: c.b }}
                      onchange={(rgb) => setColor(f, c, rgb)}
                    />
                    <button
                      class="del"
                      title="Remove color"
                      aria-label="Remove color"
                      onclick={() => removeColor(f, c)}>×</button
                    >
                  </div>
                {/each}
              </div>
            {/if}
          {/each}

          {#if f.extraKeys.length}
            <p class="preserved">
              Preserved unchanged: {f.extraKeys.join(", ")}
            </p>
          {/if}
        </section>
      {/each}

      <button class="new-file" onclick={() => createNewFile(selectedGroup)}>
        ＋ New pool file (adds to the base pool)
      </button>
    {/if}
  </div>
</OverlaySurface>

<style>
  .pool-tabs {
    display: inline-flex;
    gap: 0.25rem;
  }
  .pool-tabs button {
    background: #2b323d;
    border: 1px solid #1f242c;
    color: #cfd4db;
    font: inherit;
    padding: 0.25rem 0.7rem;
    cursor: pointer;
  }
  .pool-tabs button.active {
    background: #4a6da7;
    color: #fff;
  }

  .body {
    padding: 1rem;
    overflow-y: auto;
    max-height: 100%;
  }

  .status {
    color: #8a919c;
  }
  .status.err {
    color: #d97a7a;
  }

  .pool-note {
    color: #8a919c;
    font-size: 0.82rem;
    margin: 0 0 1rem;
    max-width: 60rem;
  }

  .file {
    border: 1px solid #1f242c;
    background: #2b323d;
    padding: 0.75rem 1rem;
    margin-bottom: 1rem;
  }

  .file header {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    margin-bottom: 0.6rem;
  }
  .file .rel {
    color: #cfd4db;
    font-size: 0.85rem;
  }

  .badge {
    font-size: 0.7rem;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    padding: 0.05rem 0.4rem;
    border: 1px solid #1f242c;
    color: #cfd4db;
  }
  .badge.base {
    background: #3f4855;
  }
  .badge.mod {
    background: #4a6da7;
    color: #fff;
  }
  .badge.new {
    background: #5a7a4a;
    color: #fff;
  }

  .scalars {
    display: flex;
    gap: 1rem;
    margin-bottom: 0.6rem;
  }
  .scalar {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    color: #cfd4db;
    font-size: 0.82rem;
  }
  .scalar input {
    width: 5rem;
    background: #21262e;
    border: 1px solid #1f242c;
    color: #cfd4db;
    font: inherit;
    padding: 0.2rem 0.35rem;
  }

  .section-head {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    margin: 0.75rem 0 0.4rem;
  }
  .section-head h4 {
    margin: 0;
    color: #cfd4db;
    font-size: 0.85rem;
    font-weight: 600;
  }
  .add,
  .new-file {
    background: #3f4855;
    border: 1px solid #1f242c;
    color: #cfd4db;
    font: inherit;
    font-size: 0.8rem;
    padding: 0.2rem 0.6rem;
    cursor: pointer;
  }
  .add:hover,
  .new-file:hover {
    background: #4a6da7;
    color: #fff;
  }
  .new-file {
    display: block;
    margin-top: 0.5rem;
  }

  .grid {
    display: flex;
    flex-wrap: wrap;
    gap: 0.35rem;
  }
  .cell {
    position: relative;
    display: inline-flex;
  }
  .cell .del {
    position: absolute;
    top: -0.4rem;
    right: -0.4rem;
    width: 1rem;
    height: 1rem;
    line-height: 0.9rem;
    text-align: center;
    padding: 0;
    background: #2b323d;
    border: 1px solid #1f242c;
    color: #d97a7a;
    font-size: 0.75rem;
    cursor: pointer;
    opacity: 0;
    z-index: 21;
  }
  .cell:hover .del {
    opacity: 1;
  }

  .preserved {
    color: #8a919c;
    font-size: 0.78rem;
    margin: 0.6rem 0 0;
  }
</style>
