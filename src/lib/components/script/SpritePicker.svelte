<!--
  SpritePicker — a searchable grid of GFX sprites (Sprint 14.4 UI). Filtered by a
  `prefix` (e.g. `GFX_mission_`, event-picture sprites), previews are LAZY-loaded:
  the index (name → texturefile) is fetched once, but each tile's PNG is only
  requested when the tile scrolls into view (IntersectionObserver), because a
  vanilla install has thousands of sprites. A hovered/selected sprite is shown at
  native size in the side preview. Sprites the backend can't decode (BC7 / other
  DX10) surface a placeholder tile carrying the error instead of garbage.

  Purity: selection is reported via `onselect(name)` (and the bindable `value`);
  the picker owns no edit state.
-->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onDestroy } from "svelte";

  /** One `spriteType` (mirrors backend `gfx::Sprite`). */
  export interface Sprite {
    name: string;
    texturefile: string;
  }

  interface Preview {
    status: "loading" | "ok" | "error";
    url?: string;
    error?: string;
  }

  let {
    installPath,
    modPath = null,
    prefix = "",
    contains = "",
    value = $bindable<string | null>(null),
    onselect,
    sprites: injectedSprites,
    loadSprite,
  }: {
    installPath?: string;
    modPath?: string | null;
    /** Case-insensitive name prefix filter (server-side). */
    prefix?: string;
    /** Case-insensitive substring the name must contain (client-side). Used for
     *  suffix-shaped families like event pictures (`*_eventPicture`) that share no
     *  common prefix the server filter can express. */
    contains?: string;
    /** Selected sprite name (bindable). */
    value?: string | null;
    onselect?: (name: string) => void;
    /** TEST/BENCH injection: sprites to show without hitting `get_sprite_index`. */
    sprites?: Sprite[];
    /** TEST/BENCH injection: per-sprite PNG loader (defaults to `get_sprite`). */
    loadSprite?: (name: string) => Promise<ArrayBuffer>;
  } = $props();

  let loaded = $state<Sprite[]>([]);
  let status = $state("");
  let query = $state("");
  let previews = $state<Record<string, Preview>>({});
  let hovered = $state<string | null>(null);

  const sprites = $derived.by(() => {
    const src = injectedSprites ?? loaded;
    const c = contains.trim().toLowerCase();
    return c ? src.filter((s) => s.name.toLowerCase().includes(c)) : src;
  });

  const doLoadSprite = $derived(
    loadSprite ??
      ((name: string) =>
        invoke<ArrayBuffer>("get_sprite", { installPath, modPath, name })),
  );

  let filtered = $derived.by(() => {
    const q = query.trim().toLowerCase();
    if (!q) return sprites;
    return sprites.filter((s) => s.name.toLowerCase().includes(q));
  });

  // The native-size preview target: hovered wins, else the current selection.
  let previewName = $derived(hovered ?? value);
  let previewEntry = $derived(previewName ? previews[previewName] : undefined);

  // --- Index load (skipped when sprites are injected) ---
  $effect(() => {
    if (injectedSprites) return;
    if (!installPath) {
      status = "No session — open a project to browse sprites.";
      return;
    }
    void loadIndex(installPath, modPath, prefix);
  });

  async function loadIndex(install: string, mod: string | null, pfx: string) {
    status = "Loading sprite index…";
    try {
      loaded = await invoke<Sprite[]>("get_sprite_index", {
        installPath: install,
        modPath: mod,
        prefixFilter: pfx || null,
      });
      status = loaded.length === 0 ? "No sprites match this prefix." : "";
    } catch (e) {
      loaded = [];
      status = `Could not load sprites: ${e}`;
    }
  }

  // --- Lazy preview loading via IntersectionObserver ---
  let observer: IntersectionObserver | null = null;
  const elNames = new WeakMap<Element, string>();

  function ensureObserver(root: Element) {
    if (observer) return;
    observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (!entry.isIntersecting) continue;
          const name = elNames.get(entry.target);
          if (name) void loadPreview(name);
        }
      },
      { root, rootMargin: "120px" },
    );
  }

  // Svelte action: observe a tile once the grid (its offsetParent) exists.
  function lazy(el: HTMLElement, name: string) {
    const root = el.closest(".grid") ?? el.parentElement ?? el;
    ensureObserver(root);
    elNames.set(el, name);
    observer?.observe(el);
    return {
      update(next: string) {
        elNames.set(el, next);
      },
      destroy() {
        observer?.unobserve(el);
      },
    };
  }

  async function loadPreview(name: string) {
    if (previews[name]) return; // loading or done
    previews = { ...previews, [name]: { status: "loading" } };
    try {
      const buf = await doLoadSprite(name);
      const url = URL.createObjectURL(new Blob([buf], { type: "image/png" }));
      previews = { ...previews, [name]: { status: "ok", url } };
    } catch (e) {
      previews = { ...previews, [name]: { status: "error", error: String(e) } };
    }
  }

  function pick(name: string) {
    value = name;
    onselect?.(name);
  }

  // Reset previews (and revoke object URLs) when the source changes.
  function revokeAll() {
    for (const p of Object.values(previews)) if (p.url) URL.revokeObjectURL(p.url);
  }
  $effect(() => {
    // Track the identity of the current source so a session/prefix change clears.
    void prefix;
    void installPath;
    void modPath;
    void injectedSprites;
    return () => {
      revokeAll();
      previews = {};
    };
  });

  onDestroy(() => {
    revokeAll();
    observer?.disconnect();
  });
</script>

<div class="sprite-picker">
  <div class="head">
    <input
      class="search"
      type="text"
      placeholder="Search sprites…"
      bind:value={query}
    />
    <span class="count">{filtered.length}</span>
  </div>

  {#if status}
    <p class="status">{status}</p>
  {/if}

  <div class="body">
    <div class="grid" role="listbox" aria-label="Sprites">
      {#each filtered as sprite (sprite.name)}
        {@const p = previews[sprite.name]}
        <button
          class="tile"
          class:selected={sprite.name === value}
          role="option"
          aria-selected={sprite.name === value}
          title={sprite.name}
          use:lazy={sprite.name}
          onclick={() => pick(sprite.name)}
          onmouseenter={() => (hovered = sprite.name)}
          onmouseleave={() => (hovered === sprite.name ? (hovered = null) : null)}
        >
          <span class="thumb">
            {#if p?.status === "ok"}
              <img src={p.url} alt={sprite.name} />
            {:else if p?.status === "error"}
              <span class="thumb-err" title={p.error}>⚠</span>
            {:else}
              <span class="thumb-load"></span>
            {/if}
          </span>
          <span class="tile-name">{sprite.name}</span>
        </button>
      {/each}
      {#if filtered.length === 0 && !status}
        <p class="empty">No sprites.</p>
      {/if}
    </div>

    <aside class="preview">
      {#if previewName}
        <div class="preview-name">{previewName}</div>
        <div class="preview-img">
          {#if previewEntry?.status === "ok"}
            <img src={previewEntry.url} alt={previewName} />
          {:else if previewEntry?.status === "error"}
            <div class="preview-err">
              Unsupported / undecodable sprite:
              <span>{previewEntry.error}</span>
            </div>
          {:else}
            <div class="preview-load">Loading…</div>
          {/if}
        </div>
      {:else}
        <div class="preview-hint">Hover or select a sprite to preview at native size.</div>
      {/if}
    </aside>
  </div>
</div>

<style>
  .sprite-picker {
    display: flex;
    flex-direction: column;
    min-height: 0;
    height: 100%;
    background: var(--bg-1);
    border: 1px solid var(--border);
    color: var(--text-1);
  }

  .head {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    flex: none;
    padding: 0.4rem;
    background: var(--bg-2);
    border-bottom: 1px solid var(--border);
  }

  .search {
    flex: 1;
    min-width: 0;
    background: var(--bg-1);
    border: 1px solid var(--border);
    color: var(--text-1);
    font-family: inherit;
    font-size: 0.85rem;
    padding: 0.3rem 0.45rem;
    outline: none;
  }

  .count {
    flex: none;
    font-size: 0.76rem;
    color: var(--text-2);
    font-variant-numeric: tabular-nums;
  }

  .status {
    margin: 0;
    padding: 0.4rem 0.5rem;
    font-size: 0.8rem;
    color: var(--text-2);
  }

  .body {
    display: flex;
    flex: 1;
    min-height: 0;
  }

  .grid {
    flex: 1;
    min-width: 0;
    overflow-y: auto;
    padding: 0.4rem;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(6rem, 1fr));
    gap: 0.4rem;
    align-content: start;
  }

  .tile {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.25rem;
    border: 1px solid var(--border);
    background: var(--bg-1);
    color: var(--text-1);
    font-family: inherit;
    padding: 0.3rem;
    cursor: pointer;
  }

  .tile:hover {
    border-color: var(--accent);
  }

  .tile.selected {
    border-color: var(--accent);
    background: rgba(74, 109, 167, 0.25);
  }

  .thumb {
    width: 100%;
    height: 3.5rem;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--bg-0);
    overflow: hidden;
  }

  .thumb img {
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
    image-rendering: pixelated;
  }

  .thumb-err {
    color: var(--warn);
    font-size: 1.3rem;
  }

  .thumb-load {
    width: 1rem;
    height: 1rem;
    border: 2px solid var(--bg-3);
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  .tile-name {
    width: 100%;
    font-size: 0.66rem;
    color: var(--text-2);
    text-align: center;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .empty {
    color: var(--text-2);
    font-size: 0.82rem;
  }

  .preview {
    flex: none;
    width: 15rem;
    border-left: 1px solid var(--border);
    background: var(--bg-0);
    padding: 0.5rem;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    overflow: auto;
  }

  .preview-name {
    font-family: "Consolas", "Courier New", monospace;
    font-size: 0.74rem;
    color: var(--text-1);
    word-break: break-all;
  }

  .preview-img {
    display: flex;
    align-items: center;
    justify-content: center;
    min-height: 6rem;
    background:
      repeating-conic-gradient(var(--bg-1) 0% 25%, var(--bg-1) 0% 50%) 50% / 16px 16px;
    padding: 0.5rem;
  }

  .preview-img img {
    max-width: 100%;
    image-rendering: pixelated;
  }

  .preview-err {
    color: var(--warn);
    font-size: 0.76rem;
  }

  .preview-err span {
    display: block;
    color: var(--text-2);
    margin-top: 0.25rem;
    word-break: break-word;
  }

  .preview-load {
    color: var(--text-2);
    font-size: 0.8rem;
  }

  .preview-hint {
    color: var(--text-2);
    font-size: 0.78rem;
  }
</style>
