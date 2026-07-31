<!--
  MultiSelectModal — a modal with search + checkbox multi-select + a mass-action
  slot. The dynasty modal (SPRINT 1.3) is the prototype: search all dynasties,
  check several, mass-delete. Confirm/cancel; substring search.

  z-index: 100 (backdrop) / 101 (dialog) — deliberately above the toolbar layer (10)
  and popover layer (20); a modal is meant to sit over everything.
-->
<script lang="ts">
  import type { Snippet } from "svelte";
  import type { MultiSelectItem } from "./types";

  let {
    open = $bindable(false),
    title,
    items,
    selected = $bindable([]),
    confirmLabel = "OK",
    searchPlaceholder = "Search…",
    onconfirm,
    oncancel,
    actions,
  }: {
    open?: boolean;
    title: string;
    items: MultiSelectItem[];
    /** Bindable array of selected keys. */
    selected?: string[];
    confirmLabel?: string;
    searchPlaceholder?: string;
    onconfirm?: (keys: string[]) => void;
    oncancel?: () => void;
    /** Mass-action slot; receives the current selection (e.g. a "Delete" button). */
    actions?: Snippet<[string[]]>;
  } = $props();

  let query = $state("");

  let filtered = $derived.by(() => {
    const q = query.trim().toLowerCase();
    if (!q) return items;
    return items.filter((i) => i.label.toLowerCase().includes(q));
  });

  let selectedSet = $derived(new Set(selected));
  let allFilteredSelected = $derived(
    filtered.length > 0 && filtered.every((i) => selectedSet.has(i.key)),
  );

  function toggle(key: string) {
    selected = selectedSet.has(key)
      ? selected.filter((k) => k !== key)
      : [...selected, key];
  }

  function toggleAllFiltered() {
    if (allFilteredSelected) {
      const remove = new Set(filtered.map((i) => i.key));
      selected = selected.filter((k) => !remove.has(k));
    } else {
      const add = filtered.map((i) => i.key);
      selected = [...new Set([...selected, ...add])];
    }
  }

  function confirm() {
    onconfirm?.(selected);
    open = false;
  }

  function cancel() {
    oncancel?.();
    open = false;
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      cancel();
      e.preventDefault();
    }
  }
</script>

{#if open}
  <div class="modal-root" role="dialog" aria-modal="true" aria-label={title}>
    <button class="backdrop" aria-label="Cancel" onclick={cancel}></button>
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <div class="dialog" onkeydown={onKeydown} role="document">
      <header class="dialog-head">
        <span class="dialog-title">{title}</span>
        <button class="close" aria-label="Close" onclick={cancel}>×</button>
      </header>

      <div class="search">
        <!-- svelte-ignore a11y_autofocus -->
        <input
          type="text"
          placeholder={searchPlaceholder}
          bind:value={query}
          autofocus
        />
        <button class="select-all" onclick={toggleAllFiltered}>
          {allFilteredSelected ? "Clear shown" : "Select shown"}
        </button>
      </div>

      <ul class="items">
        {#if filtered.length === 0}
          <li class="empty">No matches</li>
        {/if}
        {#each filtered as item (item.key)}
          <li>
            <label class="row" class:checked={selectedSet.has(item.key)}>
              <input
                type="checkbox"
                checked={selectedSet.has(item.key)}
                onchange={() => toggle(item.key)}
              />
              <span class="row-label">{item.label}</span>
              {#if item.badge !== undefined}
                <span class="row-badge">{item.badge}</span>
              {/if}
            </label>
          </li>
        {/each}
      </ul>

      <footer class="dialog-foot">
        <span class="count">{selected.length} selected</span>
        <span class="spacer"></span>
        {#if actions}
          <div class="mass-actions">{@render actions(selected)}</div>
        {/if}
        <button class="btn" onclick={cancel}>Cancel</button>
        <button class="btn primary" onclick={confirm}>{confirmLabel}</button>
      </footer>
    </div>
  </div>
{/if}

<style>
  .modal-root {
    position: fixed;
    inset: 0;
    z-index: var(--z-modal);
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .backdrop {
    position: absolute;
    inset: 0;
    border: none;
    background: rgba(0, 0, 0, 0.55);
    cursor: default;
  }

  .dialog {
    position: relative;
    z-index: var(--z-modal-content);
    display: flex;
    flex-direction: column;
    width: 26rem;
    max-width: calc(100vw - 2rem);
    max-height: calc(100vh - 4rem);
    background: var(--bg-2);
    border: 1px solid var(--border);
    color: var(--text-1);
    box-shadow: 0 8px 28px rgba(0, 0, 0, 0.5);
  }

  .dialog-head {
    display: flex;
    align-items: center;
    padding: 0.45rem 0.6rem;
    background: var(--bg-3);
    border-bottom: 1px solid var(--border);
  }

  .dialog-title {
    flex: 1;
    font-weight: 700;
  }

  .close {
    border: none;
    background: transparent;
    color: var(--text-1);
    font-size: 1.2rem;
    line-height: 1;
    cursor: pointer;
    padding: 0 0.25rem;
  }

  .close:hover {
    color: var(--text-inverse);
  }

  .search {
    display: flex;
    gap: 0.4rem;
    padding: 0.5rem 0.6rem;
    border-bottom: 1px solid var(--border);
  }

  .search input {
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

  .select-all {
    flex: none;
    border: 1px solid var(--border);
    background: var(--bg-3);
    color: var(--text-1);
    font-family: inherit;
    font-size: 0.78rem;
    padding: 0.3rem 0.5rem;
    cursor: pointer;
  }

  .select-all:hover {
    background: var(--accent);
    color: var(--text-inverse);
  }

  .items {
    list-style: none;
    margin: 0;
    padding: 0.25rem;
    overflow-y: auto;
    flex: 1;
    min-height: 4rem;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.3rem 0.45rem;
    cursor: pointer;
  }

  .row:hover {
    background: var(--bg-3);
  }

  .row.checked {
    background: rgba(74, 109, 167, 0.28);
  }

  .row-label {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .row-badge {
    flex: none;
    font-size: 0.72rem;
    color: var(--text-2);
    font-variant-numeric: tabular-nums;
  }

  .dialog-foot {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    padding: 0.5rem 0.6rem;
    background: var(--bg-2);
    border-top: 1px solid var(--border);
  }

  .count {
    font-size: 0.8rem;
    color: var(--text-2);
  }

  .spacer {
    flex: 1;
  }

  .btn {
    border: 1px solid var(--border);
    background: var(--bg-3);
    color: var(--text-1);
    font-family: inherit;
    font-size: 0.82rem;
    padding: 0.3rem 0.8rem;
    cursor: pointer;
  }

  .btn:hover {
    background: var(--accent);
    color: var(--text-inverse);
  }

  .btn.primary {
    background: var(--accent);
    color: var(--text-inverse);
    font-weight: 600;
  }

  .empty {
    padding: 0.5rem;
    color: var(--text-2);
    font-size: 0.82rem;
  }
</style>
