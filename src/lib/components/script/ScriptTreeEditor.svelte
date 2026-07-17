<!--
  ScriptTreeEditor — the Sprint 14.2 condition/effect tree editor over one script
  block (a decision `potential`/`allow`/`effect`, an event `trigger`, an MTTH
  modifier, a mission `trigger`/`effect`, …).

  ── Purity contract ───────────────────────────────────────────────────────────
  Props in, edit-batches out. The component NEVER touches the pending-edit queue;
  every mutation is reported via `onedit(edits, label)` and the HOST wraps it into
  a composite, pushes it, and re-parses the block (re-invoking parse_script_block
  against base+pending) to feed a fresh `block` back in. This mirrors the
  IdeaBlockEditor own-block-commit pattern. See scriptEdits.ts for the node →
  TypedEdit mapping (setScalar / setBlock / insertStatement / removeStatement).

  ── Raw / tree toggle ─────────────────────────────────────────────────────────
  A per-block toggle. Raw mode shows the block's braces-inclusive `raw` slice in a
  monospace editor; leaving raw mode re-validates it via `validate` (default:
  the backend `validate_script_text`). A parse error BLOCKS the switch back to the
  tree and is shown inline. A changed, valid raw edit emits ONE setBlock replacing
  the whole block (addressed by `rootPath`).
-->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import type { DropdownItem } from "$lib/components/ui";
  import type { TypedEdit } from "$lib/edits.svelte";
  import ScriptNode from "./ScriptNode.svelte";
  import ScriptAdder from "./ScriptAdder.svelte";
  import type { KnownKey, Registry, ScriptBlock, ScriptValidation } from "./scriptTypes";
  import { setWholeBlockEdit } from "./scriptEdits";

  let {
    file,
    rootPath,
    block,
    registry,
    known,
    countries = [],
    validate,
    onedit,
  }: {
    /** Game-relative file the block lives in. */
    file: string;
    /** Byte-surgical path to the edited block (e.g.
     *  ["country_decisions","form_france","potential"]). */
    rootPath: string[];
    /** Parsed block (nodes + raw + span) from `parse_script_block`. */
    block: ScriptBlock;
    /** Which registry the leaf key dropdowns draw from (labels only; both value
     *  shapes are identical). */
    registry: Registry;
    /** The known-trigger or known-effect registry (from get_known_triggers /
     *  get_known_effects). */
    known: KnownKey[];
    /** Optional tag-picker source (country list with flags/swatches) for `tag`
     *  arg-kind leaves. Falls back to a text input when empty. */
    countries?: DropdownItem[];
    /** Injectable validator (defaults to the backend validate_script_text). */
    validate?: (text: string) => Promise<ScriptValidation>;
    /** Edit-batch sink. `edits` apply in order as ONE composite labelled `label`. */
    onedit: (edits: TypedEdit[], label: string) => void;
  } = $props();

  const doValidate = $derived(
    validate ?? ((text: string) => invoke<ScriptValidation>("validate_script_text", { text })),
  );

  let knownMap = $derived(new Map(known.map((k) => [k.key, k])));

  // Raw/tree toggle state.
  let raw = $state(false);
  let rawBuffer = $state("");
  let rawError = $state<string | null>(null);

  function enterRaw() {
    rawBuffer = block.raw;
    rawError = null;
    raw = true;
  }

  async function leaveRaw() {
    rawError = null;
    const res = await doValidate(rawBuffer);
    if (!res.valid) {
      rawError = res.error ?? "Invalid script — fix before switching back";
      return; // block the toggle back to the tree
    }
    if (rawBuffer.trim() !== block.raw.trim()) {
      onedit([setWholeBlockEdit(file, rootPath, rawBuffer)], "Edit block (raw)");
    }
    raw = false;
  }
</script>

<div class="script-editor">
  <div class="toolbar">
    <span class="reg-tag" class:effects={registry === "effects"}>
      {registry === "triggers" ? "conditions" : "effects"}
    </span>
    <span class="spacer"></span>
    {#if raw}
      <button class="mode-btn" onclick={leaveRaw}>◀ Tree</button>
    {:else}
      <button class="mode-btn" onclick={enterRaw}>Raw ▶</button>
    {/if}
  </div>

  {#if raw}
    <div class="raw-wrap">
      <textarea class="raw-block" bind:value={rawBuffer} spellcheck="false"></textarea>
      {#if rawError}<div class="raw-block-error">{rawError}</div>{/if}
      <p class="raw-hint">
        Edit the raw script; leaving raw mode re-validates and commits the whole block.
      </p>
    </div>
  {:else}
    <div class="tree">
      {#if block.nodes.length === 0}
        <p class="empty">Empty block.</p>
      {/if}
      {#each block.nodes as node, i (i)}
        <ScriptNode
          {node}
          {file}
          {known}
          {knownMap}
          {countries}
          validate={doValidate}
          {onedit}
        />
      {/each}
      <div class="root-adder">
        <ScriptAdder {file} blockPath={rootPath} {known} {onedit} />
      </div>
    </div>
  {/if}
</div>

<style>
  .script-editor {
    background: #1b2027;
    border: 1px solid #1f242c;
  }

  .toolbar {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    padding: 0.3rem 0.4rem;
    background: #2b323d;
    border-bottom: 1px solid #1f242c;
  }

  .reg-tag {
    font-size: 0.68rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: #cfd4db;
    background: #3f8a6d;
    padding: 0.08rem 0.4rem;
  }

  .reg-tag.effects {
    background: #b8863b;
  }

  .spacer {
    flex: 1;
  }

  .mode-btn {
    border: 1px solid #1f242c;
    background: #3f4855;
    color: #cfd4db;
    font-family: inherit;
    font-size: 0.76rem;
    padding: 0.2rem 0.6rem;
    cursor: pointer;
  }

  .mode-btn:hover {
    background: #4a6da7;
    color: #fff;
  }

  .tree {
    padding: 0.3rem 0.35rem;
    max-height: 26rem;
    overflow-y: auto;
  }

  .root-adder {
    padding: 0.2rem 0.1rem 0.1rem;
  }

  .empty {
    margin: 0 0 0.3rem;
    color: #8a919c;
    font-size: 0.8rem;
  }

  .raw-wrap {
    padding: 0.4rem;
  }

  .raw-block {
    width: 100%;
    min-height: 12rem;
    resize: vertical;
    background: #16191f;
    border: 1px solid #1f242c;
    color: #d7dbe0;
    font-family: "Consolas", "Courier New", monospace;
    font-size: 0.8rem;
    line-height: 1.4;
    padding: 0.4rem;
    outline: none;
    white-space: pre;
    tab-size: 4;
  }

  .raw-block-error {
    margin-top: 0.3rem;
    background: #7a2e2e;
    color: #f2dede;
    border: 1px solid #a13636;
    padding: 0.25rem 0.5rem;
    font-size: 0.78rem;
  }

  .raw-hint {
    margin: 0.35rem 0 0;
    color: #8a919c;
    font-size: 0.74rem;
  }
</style>
