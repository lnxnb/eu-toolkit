<!--
  ScriptNode — one row of the script tree, recursive over `node.children`.

    • group  — a color-coded badge chip (AND/OR/NOT/scope/quantifier/…), collapse
               toggle, delete, its children (recursive), and a ScriptAdder.
    • leaf   — a label + a value input typed by the known-key registry's arg_kind
               (bool toggle / number / tag picker / string); a block-valued leaf
               or an unknown key falls back to a raw, parse-validated editor.
    • bare / anonymous — not path-addressable: shown read-only (edit via the
               block's raw/tree toggle).

  Pure: every mutation is emitted through `onedit(edits, label)`; the host owns
  the queue and re-parses. See scriptEdits.ts for the node → TypedEdit mapping.
-->
<script lang="ts">
  import { SearchDropdown } from "$lib/components/ui";
  import type { DropdownItem } from "$lib/components/ui";
  import type { TypedEdit } from "$lib/edits.svelte";
  import ScriptNode from "./ScriptNode.svelte";
  import ScriptAdder from "./ScriptAdder.svelte";
  import type { KnownKey, ScriptValidation, TreeNode } from "./scriptTypes";
  import {
    groupBadge,
    isAddressable,
    isBlockLeaf,
    removeNodeEdit,
    setBlockLeafEdit,
    setScalarEdit,
  } from "./scriptEdits";
  import { resolveScripted, jumpToScripted } from "$lib/scripted.svelte";

  let {
    node,
    file,
    known,
    knownMap,
    countries = [],
    validate,
    onedit,
    depth = 0,
  }: {
    node: TreeNode;
    file: string;
    known: KnownKey[];
    knownMap: Map<string, KnownKey>;
    countries?: DropdownItem[];
    validate: (text: string) => Promise<ScriptValidation>;
    onedit: (edits: TypedEdit[], label: string) => void;
    depth?: number;
  } = $props();

  let collapsed = $state(false);

  let knownKey = $derived(node.key ? knownMap.get(node.key) : undefined);
  let argKind = $derived(knownKey?.argKind);
  let displayLabel = $derived(knownKey?.displayName ?? node.key ?? "");
  // Sprint 28: an unmodeled leaf whose key is a scripted trigger/effect resolves
  // to a jump-link (a call site), overriding the raw fallback.
  let scriptedRef = $derived(
    node.nodeType === "leaf" && !knownKey ? resolveScripted(node.key) : undefined,
  );
  // A leaf is "raw" when it carries a block argument or its key is unmodeled AND
  // not a scripted-name reference.
  let rawLeaf = $derived(
    node.nodeType === "leaf" && !scriptedRef && (isBlockLeaf(node) || !knownKey),
  );
  let badge = $derived(node.nodeType === "group" ? groupBadge(node) : null);
  let addressable = $derived(isAddressable(node));

  function del() {
    onedit([removeNodeEdit(file, node)], `Delete ${node.key ?? "block"}`);
  }

  function setScalar(value: string) {
    if (node.value && value === node.value.text) return;
    onedit([setScalarEdit(file, node, value, argKind)], `Set ${node.key}`);
  }

  // --- Raw leaf (block-valued or unknown key) editing ---
  // Seeded from the node and RE-SEEDED whenever the node's value changes (after a
  // commit + host re-parse). The effect depends only on the incoming value, never
  // on `rawText`, so in-progress typing is never clobbered by unrelated updates.
  let rawText = $state("");
  let rawError = $state<string | null>(null);
  $effect(() => {
    rawText = node.value?.text ?? node.raw;
    rawError = null;
  });

  async function commitRaw() {
    rawError = null;
    const text = rawText.trim();
    const original = (node.value?.text ?? "").trim();
    if (text === original) return;
    // Validate the whole statement so a stray brace/quote is caught.
    const res = await validate(`${node.key} = ${text}`);
    if (!res.valid) {
      rawError = res.error ?? "Invalid script";
      return;
    }
    if (isBlockLeaf(node)) {
      onedit([setBlockLeafEdit(file, node, text)], `Edit ${node.key}`);
    } else {
      onedit([setScalarEdit(file, node, text, argKind)], `Edit ${node.key}`);
    }
  }

  let tagItems = $derived<DropdownItem[]>(countries);
</script>

<div class="node" class:group={node.nodeType === "group"} style="--depth: {depth}">
  {#if node.nodeType === "group"}
    <div class="row group-row">
      <button
        class="twist"
        aria-label={collapsed ? "Expand" : "Collapse"}
        onclick={() => (collapsed = !collapsed)}
      >
        {collapsed ? "▸" : "▾"}
      </button>
      <span class="badge" style="background: {badge?.color}">{badge?.label}</span>
      <span class="child-count">{node.children.length}</span>
      <span class="spacer"></span>
      {#if addressable}
        <button class="del" aria-label="Delete group" onclick={del}>×</button>
      {:else}
        <span class="ro-note" title="Anonymous block — edit via the raw toggle">raw</span>
      {/if}
    </div>

    {#if !collapsed}
      <div class="children">
        {#each node.children as child, i (i)}
          <ScriptNode
            node={child}
            {file}
            {known}
            {knownMap}
            {countries}
            {validate}
            {onedit}
            depth={depth + 1}
          />
        {/each}
        {#if addressable}
          <div class="group-adder">
            <ScriptAdder {file} blockPath={node.path} {known} {onedit} />
          </div>
        {/if}
      </div>
    {/if}
  {:else if !addressable}
    <!-- Bare list element / anonymous leaf: read-only raw. -->
    <div class="row leaf-row">
      <code class="raw-ro">{node.raw.trim()}</code>
      <span class="spacer"></span>
      <span class="ro-note" title="Not addressable — edit via the raw toggle">raw</span>
    </div>
  {:else if scriptedRef}
    <!-- Scripted trigger/effect call: render the name as a jump-link. -->
    <div class="row leaf-row scripted-row">
      <button
        class="scripted-link"
        title={`Jump to ${scriptedRef.kind} definition (${scriptedRef.file})`}
        onclick={() => scriptedRef && jumpToScripted(scriptedRef)}
      >
        <span class="link-kind" class:effect={scriptedRef.kind === "effect"}>
          {scriptedRef.kind === "trigger" ? "T" : "E"}
        </span>
        <span class="link-name">{node.key}</span>
        <span class="link-arrow">↗</span>
      </button>
      <span class="value grow">
        {#if isBlockLeaf(node)}
          <textarea
            class="raw-input"
            bind:value={rawText}
            onblur={commitRaw}
            spellcheck="false"
          ></textarea>
          {#if rawError}<span class="raw-error">{rawError}</span>{/if}
        {:else if node.value?.text === "yes" || node.value?.text === "no"}
          <button
            class="toggle"
            class:on={node.value?.text === "yes"}
            onclick={() => setScalar(node.value?.text === "yes" ? "no" : "yes")}
          >
            {node.value?.text}
          </button>
        {:else}
          <input
            type="text"
            value={node.value?.text ?? ""}
            onchange={(e) => setScalar(e.currentTarget.value)}
          />
        {/if}
      </span>
      <button class="del" aria-label="Delete" onclick={del}>×</button>
    </div>
  {:else if rawLeaf}
    <!-- Block-valued leaf or unmodeled key: raw, parse-validated editor. -->
    <div class="row leaf-row raw-leaf">
      <span class="key raw-key" title={node.key ?? ""}>
        {node.key}
        <span class="raw-tag">raw</span>
      </span>
      <span class="value grow">
        <textarea
          class="raw-input"
          class:multiline={isBlockLeaf(node)}
          bind:value={rawText}
          onblur={commitRaw}
          spellcheck="false"
        ></textarea>
        {#if rawError}<span class="raw-error">{rawError}</span>{/if}
      </span>
      <button class="del" aria-label="Delete" onclick={del}>×</button>
    </div>
  {:else}
    <!-- Known scalar leaf: typed value input. -->
    <div class="row leaf-row">
      <span class="key" title={node.key ?? ""}>{displayLabel}</span>
      <span class="value">
        {#if argKind === "bool"}
          <button
            class="toggle"
            class:on={node.value?.text === "yes"}
            onclick={() => setScalar(node.value?.text === "yes" ? "no" : "yes")}
          >
            {node.value?.text === "yes" ? "yes" : "no"}
          </button>
        {:else if argKind === "number" || argKind === "comparison"}
          <span class="num-wrap">
            {#if argKind === "comparison"}<span class="cmp">≥</span>{/if}
            <input
              type="number"
              step="any"
              value={node.value?.text ?? ""}
              onchange={(e) => setScalar(e.currentTarget.value)}
            />
          </span>
        {:else if argKind === "tag" && tagItems.length > 0}
          <SearchDropdown
            items={tagItems}
            value={node.value?.text ?? null}
            placeholder="Pick country…"
            onselect={(key) => setScalar(key)}
          />
        {:else}
          <input
            type="text"
            value={node.value?.text ?? ""}
            onchange={(e) => setScalar(e.currentTarget.value)}
          />
        {/if}
      </span>
      <button class="del" aria-label="Delete" onclick={del}>×</button>
    </div>
  {/if}
</div>

<style>
  .node {
    border-left: 2px solid var(--bg-2);
    margin-left: calc(var(--depth) * 0.15rem);
  }

  .node.group {
    border-left-color: var(--bg-3);
  }

  .row {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    padding: 0.15rem 0.35rem;
  }

  .row:hover {
    background: var(--bg-2);
  }

  .group-row {
    background: var(--bg-1);
  }

  .twist {
    flex: none;
    border: none;
    background: transparent;
    color: var(--text-2);
    font-size: 0.7rem;
    cursor: pointer;
    padding: 0 0.1rem;
    width: 1rem;
  }

  .badge {
    flex: none;
    color: var(--text-inverse);
    font-size: 0.68rem;
    font-weight: 700;
    letter-spacing: 0.03em;
    padding: 0.05rem 0.4rem;
    border: 1px solid rgba(0, 0, 0, 0.3);
  }

  .child-count {
    flex: none;
    font-size: 0.7rem;
    color: var(--text-2);
    font-variant-numeric: tabular-nums;
  }

  .spacer {
    flex: 1;
  }

  .children {
    padding-left: 0.6rem;
  }

  .group-adder {
    padding: 0.1rem 0.35rem 0.25rem;
  }

  .key {
    flex: 1;
    min-width: 0;
    font-size: 0.82rem;
    color: var(--text-1);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .raw-key {
    font-family: "Consolas", "Courier New", monospace;
    font-size: 0.78rem;
    color: var(--text-1);
    display: flex;
    align-items: center;
    gap: 0.25rem;
  }

  .raw-tag {
    font-size: 0.6rem;
    background: var(--accent);
    color: var(--text-inverse);
    padding: 0 0.22rem;
  }

  .value {
    flex: none;
  }

  .value.grow {
    flex: 1;
    min-width: 0;
  }

  .num-wrap {
    display: inline-flex;
    align-items: center;
    gap: 0.2rem;
  }

  .cmp {
    color: var(--text-2);
    font-size: 0.85rem;
  }

  input[type="number"],
  input[type="text"] {
    width: 7rem;
    background: var(--bg-1);
    border: 1px solid var(--border);
    color: var(--text-1);
    font-family: inherit;
    font-size: 0.82rem;
    padding: 0.18rem 0.35rem;
    outline: none;
  }

  input[type="text"] {
    width: 9rem;
  }

  .raw-input {
    width: 100%;
    min-height: 1.6rem;
    resize: vertical;
    background: var(--bg-0);
    border: 1px solid var(--border);
    color: var(--text-1);
    font-family: "Consolas", "Courier New", monospace;
    font-size: 0.78rem;
    padding: 0.2rem 0.35rem;
    outline: none;
    white-space: pre;
    overflow-wrap: normal;
  }

  .raw-input.multiline {
    min-height: 3.5rem;
  }

  .raw-error {
    display: block;
    color: var(--err);
    font-size: 0.72rem;
    margin-top: 0.15rem;
  }

  .raw-ro {
    flex: 1;
    font-family: "Consolas", "Courier New", monospace;
    font-size: 0.76rem;
    color: var(--text-2);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .ro-note {
    flex: none;
    font-size: 0.62rem;
    color: var(--text-2);
    border: 1px solid var(--bg-3);
    padding: 0 0.25rem;
  }

  .toggle {
    width: 3rem;
    border: 1px solid var(--border);
    background: var(--bg-1);
    color: var(--text-1);
    font-family: inherit;
    font-size: 0.8rem;
    padding: 0.18rem 0;
    cursor: pointer;
  }

  .toggle.on {
    background: var(--accent);
    color: var(--text-inverse);
  }

  .del {
    flex: none;
    border: none;
    background: transparent;
    color: var(--text-2);
    font-size: 1.05rem;
    line-height: 1;
    cursor: pointer;
    padding: 0 0.2rem;
  }

  .del:hover {
    color: var(--err);
  }

  .scripted-link {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 0.35rem;
    border: none;
    background: transparent;
    color: var(--accent-text);
    font-family: inherit;
    font-size: 0.82rem;
    cursor: pointer;
    padding: 0;
    text-align: left;
    overflow: hidden;
  }

  .scripted-link:hover .link-name {
    text-decoration: underline;
  }

  .link-kind {
    flex: none;
    font-size: 0.6rem;
    font-weight: 700;
    background: var(--ok);
    color: var(--text-inverse);
    padding: 0 0.28rem;
    line-height: 1.2;
  }

  .link-kind.effect {
    background: var(--warn);
  }

  .link-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .link-arrow {
    flex: none;
    color: var(--accent);
    font-size: 0.72rem;
  }
</style>
