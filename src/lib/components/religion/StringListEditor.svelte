<script lang="ts">
  // A bulk-editable list of bare string tokens (heretic list). Local state
  // initialized once from `base`; commits the whole list on every change.
  let {
    base,
    placeholder = "Add…",
    upper = true,
    oncommit,
  }: {
    base: string[];
    placeholder?: string;
    /** Uppercase entries (heretic tags are uppercase). */
    upper?: boolean;
    oncommit: (items: string[]) => void;
  } = $props();

  // svelte-ignore state_referenced_locally
  let items = $state<string[]>([...base]);
  let draft = $state("");

  function add() {
    let v = draft.trim();
    if (upper) v = v.toUpperCase();
    if (v && !items.includes(v)) {
      items = [...items, v];
      oncommit(items);
    }
    draft = "";
  }

  function remove(i: number) {
    items = items.filter((_, j) => j !== i);
    oncommit(items);
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "Enter") {
      e.preventDefault();
      add();
    }
  }
</script>

<div class="list">
  {#each items as it, i (it)}
    <span class="chip">
      {it}
      <button class="x" onclick={() => remove(i)} aria-label="Remove {it}">×</button>
    </span>
  {/each}
  <input
    class="add"
    {placeholder}
    bind:value={draft}
    onkeydown={onKey}
    onblur={add}
  />
</div>

<style>
  .list {
    display: flex;
    flex-wrap: wrap;
    gap: 0.3rem;
    align-items: center;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 0.25rem;
    padding: 0.1rem 0.4rem;
    background: #2b323d;
    border: 1px solid #4b5563;
    color: #cfd4db;
    font-size: 0.8rem;
  }
  .x {
    border: none;
    background: transparent;
    color: #9ca3af;
    cursor: pointer;
    font-size: 0.9rem;
    line-height: 1;
    padding: 0;
  }
  .x:hover {
    color: #fca5a5;
  }
  .add {
    flex: 1;
    min-width: 6rem;
    background: #14181d;
    border: 1px solid #4b5563;
    color: #cfd4db;
    font-family: inherit;
    font-size: 0.8rem;
    padding: 0.15rem 0.4rem;
  }
</style>
