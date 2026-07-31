<!--
  ModifierEditor — a typed key/value modifier editor. Each row is a {key, value}: the
  key is chosen via SearchDropdown from a supplied known-modifiers list, and the value
  input is typed by the modifier's kind:
    • percent → a number shown with a "%" suffix (stored as the game fraction, e.g. 5% ⇒ 0.05)
    • flat    → a plain number
    • boolean → a yes/no toggle (stored as "yes" / "no")
    • unknown → a free-text fallback row (key not in the known list)
  Add/remove rows; the full list is emitted on every change.

  Consumers: national ideas (1.2), religion modifiers (5.2), trade-good modifiers (7.3),
  terrain modifiers (11 stretch).
-->
<script lang="ts">
  import SearchDropdown from "./SearchDropdown.svelte";
  import type { DropdownItem, KnownModifier, ModifierRow } from "./types";

  let {
    modifiers = $bindable([]),
    known = [],
    onchange,
  }: {
    modifiers?: ModifierRow[];
    known?: KnownModifier[];
    onchange?: (modifiers: ModifierRow[]) => void;
  } = $props();

  let knownMap = $derived(new Map(known.map((k) => [k.key, k])));
  let dropdownItems = $derived<DropdownItem[]>(
    known.map((k) => ({ key: k.key, label: k.label })),
  );

  // Free-text key entry for keys not in the known list.
  let customKey = $state("");

  function emit(next: ModifierRow[]) {
    modifiers = next;
    onchange?.(next);
  }

  function kindOf(key: string): KnownModifier["kind"] | "unknown" {
    return knownMap.get(key)?.kind ?? "unknown";
  }

  function addRow(key: string) {
    if (!key || modifiers.some((m) => m.key === key)) return;
    const kind = kindOf(key);
    const value = kind === "boolean" ? "yes" : "0";
    emit([...modifiers, { key, value }]);
  }

  function addCustom() {
    const k = customKey.trim();
    if (!k) return;
    addRow(k);
    customKey = "";
  }

  function removeRow(i: number) {
    emit(modifiers.filter((_, idx) => idx !== i));
  }

  function setValue(i: number, value: string) {
    emit(modifiers.map((m, idx) => (idx === i ? { ...m, value } : m)));
  }

  // percent display <-> stored fraction (5 shown ⇄ "0.05" stored).
  function percentDisplay(stored: string): string {
    const n = Number(stored);
    if (!Number.isFinite(n)) return stored;
    return String(Math.round(n * 1000) / 10);
  }
  function percentStore(shown: string): string {
    const n = Number(shown);
    if (!Number.isFinite(n)) return "0";
    return String(Math.round(n * 10) / 1000);
  }

  function labelFor(key: string): string {
    return knownMap.get(key)?.label ?? key;
  }
</script>

<div class="modifier-editor">
  {#if modifiers.length === 0}
    <p class="empty">No modifiers.</p>
  {/if}

  {#each modifiers as row, i (row.key)}
    {@const kind = kindOf(row.key)}
    <div class="mod-row">
      <span class="key" class:unknown={kind === "unknown"} title={row.key}>
        {labelFor(row.key)}
        {#if kind === "unknown"}<span class="raw-tag">raw</span>{/if}
      </span>

      <span class="value">
        {#if kind === "boolean"}
          <button
            class="toggle"
            class:on={row.value === "yes"}
            onclick={() => setValue(i, row.value === "yes" ? "no" : "yes")}
          >
            {row.value === "yes" ? "yes" : "no"}
          </button>
        {:else if kind === "percent"}
          <span class="num-wrap">
            <input
              type="number"
              step="any"
              value={percentDisplay(row.value)}
              oninput={(e) => setValue(i, percentStore(e.currentTarget.value))}
            />
            <span class="suffix">%</span>
          </span>
        {:else if kind === "flat"}
          <input
            type="number"
            step="any"
            value={row.value}
            oninput={(e) => setValue(i, e.currentTarget.value)}
          />
        {:else}
          <input
            type="text"
            value={row.value}
            oninput={(e) => setValue(i, e.currentTarget.value)}
          />
        {/if}
      </span>

      <button class="remove" aria-label="Remove modifier" onclick={() => removeRow(i)}>
        ×
      </button>
    </div>
  {/each}

  <div class="add">
    <div class="add-known">
      <SearchDropdown
        items={dropdownItems}
        placeholder="Add modifier…"
        value={null}
        onselect={(key) => addRow(key)}
      />
    </div>
    <div class="add-custom">
      <input
        type="text"
        placeholder="custom key"
        bind:value={customKey}
        onkeydown={(e) => e.key === "Enter" && addCustom()}
      />
      <button class="add-btn" onclick={addCustom}>Add</button>
    </div>
  </div>
</div>

<style>
  .modifier-editor {
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
  }

  .empty {
    margin: 0;
    color: var(--text-2);
    font-size: 0.82rem;
  }

  .mod-row {
    display: flex;
    align-items: center;
    gap: 0.4rem;
  }

  .key {
    flex: 1;
    min-width: 0;
    font-size: 0.83rem;
    color: var(--text-1);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .key.unknown {
    font-style: italic;
    color: var(--text-1);
  }

  .raw-tag {
    font-size: 0.65rem;
    font-style: normal;
    background: var(--accent);
    color: var(--text-inverse);
    padding: 0 0.25rem;
    margin-left: 0.25rem;
  }

  .value {
    flex: none;
  }

  .num-wrap {
    display: inline-flex;
    align-items: center;
    gap: 0.2rem;
  }

  .suffix {
    font-size: 0.78rem;
    color: var(--text-2);
  }

  input[type="number"],
  input[type="text"] {
    width: 5rem;
    background: var(--bg-1);
    border: 1px solid var(--border);
    color: var(--text-1);
    font-family: inherit;
    font-size: 0.82rem;
    padding: 0.2rem 0.35rem;
    outline: none;
  }

  .toggle {
    width: 3rem;
    border: 1px solid var(--border);
    background: var(--bg-1);
    color: var(--text-1);
    font-family: inherit;
    font-size: 0.8rem;
    padding: 0.2rem 0;
    cursor: pointer;
  }

  .toggle.on {
    background: var(--accent);
    color: var(--text-inverse);
  }

  .remove {
    flex: none;
    border: none;
    background: transparent;
    color: var(--text-2);
    font-size: 1.1rem;
    line-height: 1;
    cursor: pointer;
    padding: 0 0.2rem;
  }

  .remove:hover {
    color: var(--err);
  }

  .add {
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
    margin-top: 0.3rem;
    padding-top: 0.4rem;
    border-top: 1px solid var(--border);
  }

  .add-custom {
    display: flex;
    gap: 0.35rem;
  }

  .add-custom input {
    flex: 1;
    width: auto;
  }

  .add-btn {
    flex: none;
    border: 1px solid var(--border);
    background: var(--bg-3);
    color: var(--text-1);
    font-family: inherit;
    font-size: 0.8rem;
    padding: 0.2rem 0.6rem;
    cursor: pointer;
  }

  .add-btn:hover {
    background: var(--accent);
    color: var(--text-inverse);
  }
</style>
