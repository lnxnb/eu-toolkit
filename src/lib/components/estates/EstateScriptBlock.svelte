<script lang="ts">
  // One trigger/effect block of an estate object, edited via the 14.2 tree editor.
  // Lazy-parses through parse_script_block_with_edits (reflects pending edits);
  // add/remove the block via insert/removeStatement. Mirrors govnames SchemeEditor.
  import { invoke } from "@tauri-apps/api/core";
  import { ScriptTreeEditor } from "$lib/components/script";
  import type { KnownKey, ScriptBlock } from "$lib/components/script";
  import { LoadingState, type DropdownItem } from "$lib/components/ui";
  import type { EditQueue, TypedEdit } from "$lib/edits.svelte";

  let {
    installPath,
    modPath,
    queue,
    file,
    objKey,
    name,
    registry,
    present,
    known,
    countries = [],
  }: {
    installPath: string;
    modPath: string | null;
    queue: EditQueue;
    file: string;
    objKey: string;
    name: string;
    registry: "triggers" | "effects";
    present: boolean;
    known: KnownKey[];
    countries?: DropdownItem[];
  } = $props();

  const path = $derived([objKey, name]);
  let open = $state(false);
  let block = $state<ScriptBlock | null>(null);
  let error = $state<string | null>(null);
  let token = 0;

  $effect(() => {
    void installPath;
    void modPath;
    void file;
    void objKey;
    void name;
    queue.version;
    const t = ++token;
    if (!open || !present) {
      block = null;
      return;
    }
    void reload(t);
  });

  async function reload(t: number) {
    error = null;
    try {
      const b = await invoke<ScriptBlock>("parse_script_block_with_edits", {
        installPath,
        modPath,
        file,
        path,
        edits: queue.serialize(),
      });
      if (t !== token) return;
      block = b;
    } catch (e) {
      if (t !== token) return;
      block = null;
      error = String(e);
    }
  }

  function onTreeEdit(edits: TypedEdit[], label: string) {
    if (edits.length) queue.push({ label, edits });
  }
  function addBlock() {
    queue.push({
      label: `Add ${name} to ${objKey}`,
      edits: [{ kind: "insertStatement", file, blockPath: [objKey], statement: `${name} = {\n}` }],
    });
    open = true;
  }
  function removeBlock() {
    if (!confirm(`Remove ${name} block from "${objKey}"?`)) return;
    queue.push({
      label: `Remove ${name} from ${objKey}`,
      edits: [{ kind: "removeStatement", file, blockPath: [objKey], key: name }],
    });
  }
</script>

<div class="sb">
  <div class="sb-head">
    <button class="sb-title" onclick={() => (open = !open)} disabled={!present}>
      <span class="caret">{present ? (open ? "▾" : "▸") : "·"}</span>
      <code>{name}</code>
      <span class="reg">{registry === "triggers" ? "trigger" : "effect"}</span>
    </button>
    {#if present}
      <button class="mini danger" onclick={removeBlock}>remove</button>
    {:else}
      <button class="mini" onclick={addBlock}>＋ add</button>
    {/if}
  </div>
  {#if present && open}
    {#if error}
      <p class="err">{error}</p>
    {:else if block}
      <ScriptTreeEditor
        {file}
        rootPath={path}
        {block}
        {registry}
        {known}
        {countries}
        onedit={onTreeEdit}
      />
    {:else}
      <LoadingState label="Loading script block…" />
    {/if}
  {/if}
</div>

<style>
  .sb {
    border: 1px solid var(--bg-1);
    margin-top: 0.25rem;
  }
  .sb-head {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    padding: 0.15rem 0.3rem;
    background: var(--bg-1);
  }
  .sb-title {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    flex: 1;
    text-align: left;
    border: none;
    background: transparent;
    color: var(--text-1);
    font-family: inherit;
    font-size: 0.8rem;
    cursor: pointer;
  }
  .sb-title:disabled {
    cursor: default;
    color: var(--text-2);
  }
  .caret {
    color: var(--text-2);
    width: 0.8rem;
    flex: none;
  }
  code {
    color: var(--ok);
    background: var(--bg-0);
    padding: 0 0.3rem;
    font-size: 0.76rem;
  }
  .reg {
    font-size: 0.66rem;
    text-transform: uppercase;
    letter-spacing: 0.03em;
    color: var(--text-2);
  }
  .mini {
    border: 1px solid var(--border-strong);
    background: var(--bg-2);
    color: var(--text-1);
    font-family: inherit;
    font-size: 0.7rem;
    padding: 0.05rem 0.35rem;
    cursor: pointer;
    flex: none;
  }
  .mini:hover {
    border-color: var(--accent);
    background: var(--accent);
    color: var(--text-inverse);
  }
  .mini.danger {
    color: var(--err);
    border-color: var(--danger-bg);
  }
  .err {
    color: var(--err);
    font-size: 0.76rem;
    padding: 0.2rem 0.4rem;
  }
</style>
