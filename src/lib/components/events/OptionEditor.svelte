<!--
  OptionEditor — one `option = { name = "…" ai_chance = { … } <effects> }` of an
  event (Sprint 16).

  Re-parses its own option block (folding the pending queue), then presents:
    • the option NAME as a loc-text edit (LocOverride on the name loc key)
    • `ai_chance` shown raw-preserved (advanced), like a decision's ai_will_do
    • the option EFFECTS as a ScriptTreeEditor over the remaining nodes (name +
      ai_chance filtered out of the tree, so each is edited in exactly one place;
      the raw toggle still round-trips the whole block).
  Add/remove of the option itself is the parent's concern.
-->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { ScriptTreeEditor } from "$lib/components/script";
  import type { KnownKey, ScriptBlock } from "$lib/components/script";
  import type { DropdownItem } from "$lib/components/ui";
  import type { EditQueue, TypedEdit } from "$lib/edits.svelte";

  let {
    file,
    optionPath,
    installPath,
    modPath = null,
    queue,
    effects,
    countries = [],
    index,
    onedit,
    onremove,
  }: {
    file: string;
    optionPath: string[];
    installPath: string;
    modPath?: string | null;
    queue: EditQueue;
    effects: KnownKey[];
    countries?: DropdownItem[];
    /** 1-based option number for the header. */
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
    void optionPath.join("/");
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
        path: optionPath,
        edits: queue.serialize(),
      });
      if (token === loadToken) block = b;
    } catch {
      if (token === loadToken) block = null;
    }
  }

  const nameNode = $derived(block?.nodes.find((n) => n.key === "name"));
  const nameKey = $derived(nameNode?.value?.text ?? null);
  const aiChanceNode = $derived(block?.nodes.find((n) => n.key === "ai_chance"));
  const aiChanceRaw = $derived(aiChanceNode?.value?.text ?? aiChanceNode?.raw ?? null);

  // Effect tree = everything except the name + ai_chance nodes.
  const effectBlock = $derived<ScriptBlock | null>(
    block
      ? { ...block, nodes: block.nodes.filter((n) => n.key !== "name" && n.key !== "ai_chance") }
      : null,
  );

  // Name loc field (pending override wins, else the base loc for the name key).
  const nameLoc = $derived(nameKey ? (queue.pendingLocOverride(nameKey) ?? "") : "");
  function commitName(value: string) {
    if (!nameKey || value === nameLoc) return;
    onedit([{ kind: "locOverride", key: nameKey, value }], "Edit option text");
  }
</script>

<div class="opt">
  <div class="opthead">
    <span class="optidx">Option {index}</span>
    {#if nameKey}
      <code class="namekey">{nameKey}</code>
    {/if}
    <button class="rm" title="Remove this option" onclick={onremove}>✕</button>
  </div>

  {#if nameKey}
    <label class="fld">
      <span>Option text</span>
      <input
        type="text"
        value={nameLoc}
        onchange={(e) => commitName(e.currentTarget.value)}
        placeholder={nameKey}
      />
    </label>
  {/if}

  {#if effectBlock}
    <div class="eff">
      <span class="sublbl">Effects</span>
      <ScriptTreeEditor
        {file}
        rootPath={optionPath}
        block={effectBlock}
        registry="effects"
        known={effects}
        {countries}
        {onedit}
      />
    </div>
  {:else}
    <p class="loading">Loading option…</p>
  {/if}

  {#if aiChanceRaw}
    <div class="ai">
      <span class="sublbl">ai_chance <span class="adv">advanced — raw-preserved</span></span>
      <pre class="raw">{aiChanceRaw}</pre>
    </div>
  {/if}
</div>

<style>
  .opt {
    border: 1px solid #1f242c;
    background: #191d23;
    padding: 0.4rem;
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
  }

  .opthead {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .optidx {
    font-size: 0.72rem;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: #9aa2ad;
  }

  .namekey {
    color: #9aecc0;
    background: #16191f;
    padding: 0 0.3rem;
    font-size: 0.72rem;
  }

  .rm {
    margin-left: auto;
    border: 1px solid #1f242c;
    background: #3f4855;
    color: #cfd4db;
    font-family: inherit;
    font-size: 0.78rem;
    padding: 0.15rem 0.45rem;
    cursor: pointer;
  }

  .rm:hover {
    background: #a13636;
    color: #fff;
  }

  .fld {
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
    font-size: 0.78rem;
    color: #9aa2ad;
  }

  .fld input {
    background: #21262e;
    border: 1px solid #1f242c;
    color: #cfd4db;
    font-family: inherit;
    font-size: 0.85rem;
    padding: 0.22rem 0.35rem;
  }

  .sublbl {
    font-size: 0.72rem;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: #9aa2ad;
  }

  .adv {
    text-transform: none;
    letter-spacing: 0;
    color: #6d7683;
    font-size: 0.72rem;
  }

  .eff,
  .ai {
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
  }

  .raw {
    margin: 0;
    background: #16191f;
    border: 1px solid #1f242c;
    color: #9aecc0;
    font-size: 0.75rem;
    padding: 0.4rem 0.5rem;
    overflow-x: auto;
  }

  .loading {
    margin: 0;
    font-size: 0.78rem;
    color: #8a919c;
  }
</style>
