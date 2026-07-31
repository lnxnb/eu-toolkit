<!--
  SearchDropdown — a searchable single-select combobox. The text input filters the
  items (case-insensitive substring on the label); Up/Down move the highlight, Enter
  selects, Esc closes. Items carry an optional color swatch and/or leading icon, so
  tag / culture / religion pickers are literally this component with different data.

  z-index: the option list is a popover at 20 — above the panel surface (10) it lives
  in, below modals (100).
-->
<script lang="ts">
  import type { DropdownItem } from "./types";

  let {
    items,
    value = $bindable(null),
    placeholder = "Search…",
    disabled = false,
    onselect,
  }: {
    items: DropdownItem[];
    /** Selected item key (bindable). */
    value?: string | null;
    placeholder?: string;
    disabled?: boolean;
    onselect?: (key: string, item: DropdownItem) => void;
  } = $props();

  let open = $state(false);
  let query = $state("");
  let highlighted = $state(0);
  let inputEl = $state<HTMLInputElement | null>(null);

  let selectedItem = $derived(items.find((i) => i.key === value) ?? null);

  let filtered = $derived.by(() => {
    const q = query.trim().toLowerCase();
    if (!q) return items;
    return items.filter((i) => i.label.toLowerCase().includes(q));
  });

  // What the input shows: the live query while open, else the selected label.
  let displayValue = $derived(open ? query : (selectedItem?.label ?? ""));

  function openList() {
    if (disabled) return;
    open = true;
    query = "";
    highlighted = Math.max(
      0,
      filtered.findIndex((i) => i.key === value),
    );
  }

  function close() {
    open = false;
    query = "";
  }

  function choose(item: DropdownItem) {
    value = item.key;
    onselect?.(item.key, item);
    close();
    inputEl?.blur();
  }

  function onInput(e: Event) {
    query = (e.target as HTMLInputElement).value;
    open = true;
    highlighted = 0;
  }

  function onKeydown(e: KeyboardEvent) {
    if (!open && (e.key === "ArrowDown" || e.key === "Enter")) {
      openList();
      e.preventDefault();
      return;
    }
    if (!open) return;
    if (e.key === "ArrowDown") {
      highlighted = Math.min(highlighted + 1, filtered.length - 1);
      e.preventDefault();
    } else if (e.key === "ArrowUp") {
      highlighted = Math.max(highlighted - 1, 0);
      e.preventDefault();
    } else if (e.key === "Enter") {
      const item = filtered[highlighted];
      if (item) choose(item);
      e.preventDefault();
    } else if (e.key === "Escape") {
      close();
      e.preventDefault();
    }
  }
</script>

<div class="search-dropdown" class:disabled>
  <div class="control">
    {#if selectedItem?.swatch && !open}
      <span class="swatch" style="background: {selectedItem.swatch}"></span>
    {/if}
    {#if selectedItem?.icon && !open}
      <img class="icon" src={selectedItem.icon} alt="" />
    {/if}
    <input
      bind:this={inputEl}
      class="input"
      type="text"
      {placeholder}
      {disabled}
      value={displayValue}
      role="combobox"
      aria-expanded={open}
      aria-controls="search-dropdown-list"
      oninput={onInput}
      onfocus={openList}
      onblur={close}
      onkeydown={onKeydown}
    />
    <span class="caret" aria-hidden="true">▾</span>
  </div>

  {#if open}
    <ul class="list" id="search-dropdown-list" role="listbox">
      {#if filtered.length === 0}
        <li class="empty">No matches</li>
      {/if}
      {#each filtered as item, i (item.key)}
        <li>
          <button
            class="option"
            class:highlighted={i === highlighted}
            class:selected={item.key === value}
            role="option"
            aria-selected={item.key === value}
            onmousedown={(e) => e.preventDefault()}
            onmouseenter={() => (highlighted = i)}
            onclick={() => choose(item)}
          >
            {#if item.swatch}
              <span class="swatch" style="background: {item.swatch}"></span>
            {/if}
            {#if item.icon}
              <img class="icon" src={item.icon} alt="" />
            {/if}
            <span class="label">{item.label}</span>
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .search-dropdown {
    position: relative;
    width: 100%;
  }

  .control {
    display: flex;
    align-items: center;
    gap: 0.35rem;
    background: var(--bg-1);
    border: 1px solid var(--border);
    padding: 0.2rem 0.4rem;
  }

  .search-dropdown.disabled .control {
    opacity: 0.55;
  }

  .input {
    flex: 1;
    min-width: 0;
    border: none;
    background: transparent;
    color: var(--text-1);
    font-family: inherit;
    font-size: 0.85rem;
    outline: none;
  }

  .input::placeholder {
    color: var(--text-2);
  }

  .caret {
    flex: none;
    color: var(--text-2);
    font-size: 0.7rem;
    pointer-events: none;
  }

  .list {
    position: absolute;
    z-index: 20;
    top: 100%;
    left: 0;
    right: 0;
    margin: 1px 0 0;
    padding: 2px;
    list-style: none;
    max-height: 14rem;
    overflow-y: auto;
    background: var(--bg-3);
    border: 1px solid var(--border);
    box-shadow: 2px 3px 8px rgba(0, 0, 0, 0.4);
  }

  .option {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    width: 100%;
    text-align: left;
    border: none;
    background: transparent;
    color: var(--text-1);
    font-family: inherit;
    font-size: 0.85rem;
    padding: 0.3rem 0.45rem;
    cursor: pointer;
  }

  .option.highlighted {
    background: var(--accent);
    color: var(--text-inverse);
  }

  .option.selected {
    font-weight: 600;
  }

  .label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .swatch {
    flex: none;
    width: 0.8rem;
    height: 0.8rem;
    border: 1px solid var(--border);
    display: inline-block;
  }

  .icon {
    flex: none;
    width: 1.1rem;
    height: 1.1rem;
    object-fit: cover;
    border: 1px solid var(--border);
  }

  .empty {
    padding: 0.35rem 0.45rem;
    color: var(--text-2);
    font-size: 0.82rem;
  }
</style>
