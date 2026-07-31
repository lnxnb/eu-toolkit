<!--
  ModifierRow — one `modifier = { factor = X <trigger> }` row of an event's
  mean_time_to_happen block (Sprint 16).

  Re-parses its own modifier block through parse_script_block_with_edits (folding
  the pending queue), then splits it into a `factor` number field and a trigger
  ScriptTreeEditor over the REMAINING conditions (the factor node is filtered out
  of the tree so it isn't editable in two places — the raw toggle still shows the
  whole block). Add/remove of the row itself is the parent's concern; this row
  only edits its factor + conditions and reports a remove request.
-->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { ScriptTreeEditor } from "$lib/components/script";
  import type { KnownKey, ScriptBlock } from "$lib/components/script";
  import type { DropdownItem } from "$lib/components/ui";
  import type { EditQueue, TypedEdit } from "$lib/edits.svelte";

  let {
    file,
    modifierPath,
    installPath,
    modPath = null,
    queue,
    triggers,
    countries = [],
    index,
    onedit,
    onremove,
  }: {
    file: string;
    /** Byte-surgical path to this `modifier` block (occurrence-qualified). */
    modifierPath: string[];
    installPath: string;
    modPath?: string | null;
    queue: EditQueue;
    triggers: KnownKey[];
    countries?: DropdownItem[];
    /** 1-based row number for the header. */
    index: number;
    onedit: (edits: TypedEdit[], label: string) => void;
    onremove: () => void;
  } = $props();

  let block = $state<ScriptBlock | null>(null);

  let loadToken = 0;
  $effect(() => {
    void installPath;
    void modPath;
    void file;
    void modifierPath.join("/");
    queue.version;
    const token = ++loadToken;
    void reload(token);
  });

  async function reload(token: number) {
    try {
      const b = await invoke<ScriptBlock>("parse_script_block_with_edits", {
        installPath,
        modPath,
        file,
        path: modifierPath,
        edits: queue.serialize(),
      });
      if (token === loadToken) block = b;
    } catch {
      if (token === loadToken) block = null;
    }
  }

  const factorNode = $derived(block?.nodes.find((n) => n.key === "factor"));
  const factorValue = $derived(factorNode?.value?.text ?? "1");

  // The trigger tree = every node EXCEPT factor (factor gets its own field).
  const triggerBlock = $derived<ScriptBlock | null>(
    block ? { ...block, nodes: block.nodes.filter((n) => n.key !== "factor") } : null,
  );

  function commitFactor(value: string) {
    if (value === factorValue) return;
    const edit: TypedEdit = factorNode
      ? { kind: "setScalar", file, path: [...modifierPath, "factor"], value, quoted: false }
      : { kind: "insertStatement", file, blockPath: modifierPath, statement: `factor = ${value}` };
    onedit([edit], "Edit modifier factor");
  }
</script>

<div class="modrow">
  <div class="modhead">
    <span class="modidx">Modifier {index}</span>
    <label class="factor">
      factor
      <input
        type="number"
        step="0.01"
        value={factorValue}
        onchange={(e) => commitFactor(e.currentTarget.value)}
      />
    </label>
    <button class="rm" title="Remove this modifier" onclick={onremove}>✕</button>
  </div>
  {#if triggerBlock}
    <ScriptTreeEditor
      {file}
      rootPath={modifierPath}
      block={triggerBlock}
      registry="triggers"
      known={triggers}
      {countries}
      {onedit}
    />
  {:else}
    <p class="loading">Loading modifier…</p>
  {/if}
</div>

<style>
  .modrow {
    border: 1px solid var(--border);
    background: var(--bg-1);
    padding: 0.35rem;
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
  }

  .modhead {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .modidx {
    font-size: 0.72rem;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-2);
  }

  .factor {
    display: flex;
    align-items: center;
    gap: 0.3rem;
    font-size: 0.78rem;
    color: var(--text-1);
  }

  .factor input {
    width: 5rem;
    background: var(--bg-1);
    border: 1px solid var(--border);
    color: var(--text-1);
    font-family: inherit;
    font-size: 0.82rem;
    padding: 0.15rem 0.3rem;
  }

  .rm {
    margin-left: auto;
    border: 1px solid var(--border);
    background: var(--bg-3);
    color: var(--text-1);
    font-family: inherit;
    font-size: 0.78rem;
    padding: 0.15rem 0.45rem;
    cursor: pointer;
  }

  .rm:hover {
    background: var(--err);
    color: var(--text-inverse);
  }

  .loading {
    margin: 0;
    font-size: 0.78rem;
    color: var(--text-2);
  }
</style>
