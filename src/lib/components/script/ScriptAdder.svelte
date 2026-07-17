<!--
  ScriptAdder — the shared "＋ Condition / ＋ Group" affordance used at the root of
  a ScriptTreeEditor and inside every group node. Given a target `blockPath`, it
  emits a single insertStatement edit (condition or empty logical group). Pure:
  it never touches the queue — it calls `onedit(edits, label)` and the host owns
  the composite/queue.
-->
<script lang="ts">
  import { SearchDropdown } from "$lib/components/ui";
  import type { DropdownItem } from "$lib/components/ui";
  import type { TypedEdit } from "$lib/edits.svelte";
  import type { KnownKey } from "./scriptTypes";
  import { GROUP_COMBINATORS, insertConditionEdit, insertGroupEdit } from "./scriptEdits";

  let {
    file,
    blockPath,
    known,
    onedit,
  }: {
    file: string;
    blockPath: string[];
    known: KnownKey[];
    onedit: (edits: TypedEdit[], label: string) => void;
  } = $props();

  let mode = $state<"idle" | "condition" | "group">("idle");

  let argKindByKey = $derived(new Map(known.map((k) => [k.key, k.argKind])));
  let items = $derived<DropdownItem[]>(
    known.map((k) => ({ key: k.key, label: `${k.displayName} — ${k.key}` })),
  );

  function addCondition(key: string) {
    onedit([insertConditionEdit(file, blockPath, key, argKindByKey.get(key))], `Add ${key}`);
    mode = "idle";
  }

  function addGroup(combinator: string) {
    onedit([insertGroupEdit(file, blockPath, combinator)], `Add ${combinator} group`);
    mode = "idle";
  }
</script>

<div class="adder">
  {#if mode === "idle"}
    <button class="add-btn" onclick={() => (mode = "condition")}>＋ Condition</button>
    <button class="add-btn" onclick={() => (mode = "group")}>＋ Group</button>
  {:else if mode === "condition"}
    <div class="picker">
      <SearchDropdown
        {items}
        value={null}
        placeholder="Search condition…"
        onselect={(key) => addCondition(key)}
      />
      <button class="cancel" aria-label="Cancel" onclick={() => (mode = "idle")}>×</button>
    </div>
  {:else}
    <div class="combos">
      {#each GROUP_COMBINATORS as c (c)}
        <button class="combo-btn" onclick={() => addGroup(c)}>{c}</button>
      {/each}
      <button class="cancel" aria-label="Cancel" onclick={() => (mode = "idle")}>×</button>
    </div>
  {/if}
</div>

<style>
  .adder {
    display: flex;
    align-items: center;
    gap: 0.35rem;
    margin-top: 0.2rem;
  }

  .add-btn,
  .combo-btn {
    border: 1px solid #1f242c;
    background: #3f4855;
    color: #cfd4db;
    font-family: inherit;
    font-size: 0.76rem;
    padding: 0.18rem 0.5rem;
    cursor: pointer;
  }

  .add-btn:hover,
  .combo-btn:hover {
    background: #4a6da7;
    color: #fff;
  }

  .picker {
    display: flex;
    align-items: center;
    gap: 0.3rem;
    flex: 1;
    min-width: 0;
    max-width: 22rem;
  }

  .picker :global(.search-dropdown) {
    flex: 1;
  }

  .combos {
    display: flex;
    align-items: center;
    gap: 0.25rem;
    flex-wrap: wrap;
  }

  .cancel {
    flex: none;
    border: none;
    background: transparent;
    color: #8a919c;
    font-size: 1rem;
    line-height: 1;
    cursor: pointer;
    padding: 0 0.2rem;
  }

  .cancel:hover {
    color: #fca5a5;
  }
</style>
