<!--
  EditsPanel (View ▸ Edits) — Sprint 30.1.

  A left-docked, toggleable panel listing the pending edit queue chronologically
  (one row per composite), expandable to the typed edits inside. Row actions:
    • jump to target — best-effort map-mode/selection switch to the affected entity
    • undo to here   — rewinds the linear queue to before this composite (always safe)
    • revert alone    — drops just this composite, ONLY when provably independent
                        (no later composite touches the same file); otherwise the
                        button is disabled with an explanatory tooltip.

  Live over `queue.version`; the dirty count in the header mirrors the File menu.
  Post-save, MapView captures the just-saved composites into `saved` (grayed,
  read-only, clearable) so you can still scroll back through what landed.

  Docked left of the map-modes panel; z-index 10 (docked side-panel layer,
  AGENTS.md CSS stacking) in Windows-classic chrome.
-->
<script lang="ts">
  import type { EditQueue, Composite } from "$lib/edits.svelte";
  import {
    isIndependentlyRevertible,
    compositeJump,
    summarizeEdit,
    type EditJump,
  } from "$lib/editsPanel";

  let {
    queue,
    saved = [],
    onjump,
    onclose,
    onclearsaved,
  }: {
    queue: EditQueue;
    /** Session "saved" history captured at each save (grayed, read-only). */
    saved?: Composite[];
    onjump?: (jump: EditJump) => void;
    onclose?: () => void;
    onclearsaved?: () => void;
  } = $props();

  // Track expansion by composite identity so it survives queue reordering.
  let expanded = $state<Set<Composite>>(new Set());

  // Recompute the row model on every queue mutation.
  const rows = $derived.by(() => {
    void queue.version;
    const comps = queue.composites;
    return comps.map((c, i) => ({
      composite: c,
      index: i,
      jump: compositeJump(c),
      revertible: isIndependentlyRevertible(comps, i),
    }));
  });

  const dirtyCount = $derived.by(() => {
    void queue.version;
    return queue.composites.length;
  });

  function toggle(c: Composite) {
    if (expanded.has(c)) expanded.delete(c);
    else expanded.add(c);
    expanded = new Set(expanded);
  }

  function doJump(j: EditJump | null) {
    if (j) onjump?.(j);
  }

  function undoToHere(c: Composite) {
    queue.undoToBefore(c);
  }

  function revertAlone(c: Composite) {
    queue.revertComposite(c);
  }

  function revertTooltip(revertible: boolean): string {
    return revertible
      ? "Revert only this edit (no later edit depends on it)"
      : "Can't revert alone — a later edit touches the same file. Use “Undo to here”.";
  }

  function jumpTitle(j: EditJump | null): string {
    if (!j) return "No jump target for this edit";
    if (j.kind === "province") return `Jump to province ${j.id}`;
    if (j.kind === "country") return `Open country ${j.tag}`;
    return `Switch to ${j.mode} mode`;
  }
</script>

<aside class="edits-panel">
  <div class="chrome">
    <div class="titlebar">
      <span class="title">Edits</span>
      <span class="dirty" class:none={dirtyCount === 0} title="Unsaved edits">
        {dirtyCount}
      </span>
      {#if onclose}
        <button class="close" onclick={onclose} aria-label="Close panel">×</button>
      {/if}
    </div>
  </div>

  <div class="body">
    {#if rows.length === 0}
      <p class="empty">No pending edits.</p>
    {:else}
      <ul class="list">
        {#each rows as row (row.composite)}
          <li class="row">
            <div class="row-head">
              <button
                class="expander"
                onclick={() => toggle(row.composite)}
                aria-expanded={expanded.has(row.composite)}
                title={expanded.has(row.composite) ? "Collapse" : "Expand edits"}
              >
                <span class="caret">{expanded.has(row.composite) ? "▾" : "▸"}</span>
                <span class="label">{row.composite.label}</span>
                {#if row.composite.date}<span class="date-tag" title="Made at this date">{row.composite.date}</span>{/if}
                <span class="count">{row.composite.edits.length}</span>
              </button>
            </div>

            <div class="actions">
              <button
                class="act"
                disabled={!row.jump}
                title={jumpTitle(row.jump)}
                onclick={() => doJump(row.jump)}>Jump</button
              >
              <button
                class="act"
                title="Undo everything back to before this edit (reversible via Redo)"
                onclick={() => undoToHere(row.composite)}>Undo to here</button
              >
              <button
                class="act"
                disabled={!row.revertible}
                title={revertTooltip(row.revertible)}
                onclick={() => revertAlone(row.composite)}>Revert alone</button
              >
            </div>

            {#if expanded.has(row.composite)}
              <ul class="edits">
                {#each row.composite.edits as e}
                  {@const s = summarizeEdit(e)}
                  <li class="edit">
                    <span class="edit-file" title={s.file}>{s.file}</span>
                    <span class="edit-detail">{s.detail}</span>
                  </li>
                {/each}
              </ul>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}

    {#if saved.length > 0}
      <div class="saved-head">
        <span class="saved-title">Saved this session ({saved.length})</span>
        {#if onclearsaved}
          <button class="clear" onclick={onclearsaved} title="Clear saved history">Clear</button>
        {/if}
      </div>
      <ul class="list saved">
        {#each saved as c, i (i)}
          <li class="row saved-row">
            <div class="row-head">
              <span class="caret saved-caret">•</span>
              <span class="label">{c.label}</span>
              {#if c.date}<span class="date-tag">{c.date}</span>{/if}
              <span class="count">{c.edits.length}</span>
            </div>
          </li>
        {/each}
      </ul>
    {/if}
  </div>
</aside>

<style>
  .edits-panel {
    position: absolute;
    top: 3rem;
    /* Docked just right of the 12rem map-modes panel (left: 0.75rem). */
    left: 13.25rem;
    bottom: 0.75rem;
    width: 20rem;
    z-index: 10;
    display: flex;
    flex-direction: column;
    background: #2b323d;
    border: 1px solid #1f242c;
    color: #cfd4db;
    font-size: 0.85rem;
    box-shadow: 2px 3px 10px rgba(0, 0, 0, 0.4);
  }

  .chrome {
    flex: none;
    background: #3f4855;
    border-bottom: 1px solid #1f242c;
  }

  .titlebar {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.4rem 0.6rem;
  }

  .title {
    flex: 1;
    font-weight: 700;
  }

  .dirty {
    flex: none;
    min-width: 1.3rem;
    text-align: center;
    padding: 0.05rem 0.4rem;
    background: #4a6da7;
    color: #fff;
    font-variant-numeric: tabular-nums;
    font-size: 0.75rem;
    border: 1px solid rgba(0, 0, 0, 0.35);
  }

  .dirty.none {
    background: #333b46;
    color: #8a919c;
  }

  .close {
    flex: none;
    border: none;
    background: transparent;
    color: #cfd4db;
    font-size: 1.2rem;
    line-height: 1;
    padding: 0 0.25rem;
    cursor: pointer;
  }
  .close:hover {
    color: #fff;
  }

  .body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 0.4rem 0.5rem;
  }

  .empty {
    color: #8a919c;
    font-size: 0.8rem;
    padding: 0.5rem 0.2rem;
  }

  .list {
    list-style: none;
    margin: 0;
    padding: 0;
  }

  .row {
    border: 1px solid #1f242c;
    background: #262c35;
    margin-bottom: 0.3rem;
  }

  .row-head {
    display: flex;
  }

  .expander {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    flex: 1;
    min-width: 0;
    border: none;
    background: transparent;
    color: #cfd4db;
    font: inherit;
    text-align: left;
    padding: 0.3rem 0.4rem;
    cursor: pointer;
  }
  .expander:hover {
    background: #333b46;
  }

  .caret {
    flex: none;
    width: 0.8rem;
    color: #8a919c;
  }

  .label {
    flex: 1;
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .date-tag {
    flex: none;
    font-size: 0.68rem;
    color: #9aa2ad;
    background: #1f242c;
    padding: 0 0.3rem;
    font-variant-numeric: tabular-nums;
  }

  .count {
    flex: none;
    font-size: 0.7rem;
    color: #8a919c;
    background: #1f242c;
    padding: 0 0.35rem;
    min-width: 1.1rem;
    text-align: center;
  }

  .actions {
    display: flex;
    gap: 1px;
    padding: 0 0.4rem 0.3rem;
  }

  .act {
    border: 1px solid #3a434f;
    background: #2b323d;
    color: #cfd4db;
    font-family: inherit;
    font-size: 0.7rem;
    padding: 0.12rem 0.45rem;
    cursor: pointer;
  }
  .act:hover:not(:disabled) {
    background: #4a6da7;
    border-color: #4a6da7;
    color: #fff;
  }
  .act:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .edits {
    list-style: none;
    margin: 0;
    padding: 0.1rem 0.4rem 0.4rem 1.2rem;
    border-top: 1px solid #1f242c;
  }

  .edit {
    display: flex;
    flex-direction: column;
    gap: 0.05rem;
    padding: 0.2rem 0;
    border-bottom: 1px solid #21262e;
  }
  .edit:last-child {
    border-bottom: none;
  }

  .edit-file {
    font-size: 0.68rem;
    color: #9aecc0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .edit-detail {
    font-size: 0.74rem;
    color: #cfd4db;
    word-break: break-word;
  }

  .saved-head {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    margin: 0.5rem 0 0.3rem;
    padding: 0.2rem 0.2rem;
    border-top: 1px solid #1f242c;
  }

  .saved-title {
    flex: 1;
    font-size: 0.72rem;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: #8a919c;
  }

  .clear {
    border: 1px solid #3a434f;
    background: #2b323d;
    color: #cfd4db;
    font-family: inherit;
    font-size: 0.7rem;
    padding: 0.1rem 0.5rem;
    cursor: pointer;
  }
  .clear:hover {
    background: #4a6da7;
    border-color: #4a6da7;
    color: #fff;
  }

  .saved-row {
    opacity: 0.6;
  }
  .saved-row .row-head {
    align-items: center;
    padding: 0.25rem 0.4rem;
    gap: 0.4rem;
  }
  .saved-caret {
    text-align: center;
  }
</style>
