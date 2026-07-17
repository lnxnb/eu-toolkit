<!--
  NamingRuleRow — Sprint 19.2 one `names = { trigger + name }` rule.

  File order is match order (first match wins — verified against the vanilla
  file's own "# Specific" / "# Generic" ordering comments), so the row shows its
  position, up/down reorder, and a delete. The condition is the 14.2 tree editor
  over the rule's `trigger` sub-block (re-parsed through parse_script_block_with_edits
  so it reflects PENDING edits); the result name is a loc key whose display string
  edits a loc override. All edits are emitted to the shared queue by the host.
-->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { ScriptTreeEditor } from "$lib/components/script";
  import type { KnownKey, ScriptBlock } from "$lib/components/script";
  import type { DropdownItem } from "$lib/components/ui";
  import type { EditQueue, TypedEdit } from "$lib/edits.svelte";
  import type { NamingRule } from "$lib/colonial";

  let {
    installPath,
    modPath,
    queue,
    file,
    entryKey,
    rule,
    ruleCount,
    triggers,
    countries = [],
    onmove,
    onremove,
  }: {
    installPath: string;
    modPath: string | null;
    queue: EditQueue;
    file: string;
    entryKey: string;
    rule: NamingRule;
    ruleCount: number;
    triggers: KnownKey[];
    countries?: DropdownItem[];
    /** Reorder this rule by delta (−1 up, +1 down). */
    onmove: (delta: number) => void;
    onremove: () => void;
  } = $props();

  const idx = $derived(rule.index);
  const triggerPath = $derived([entryKey, `names#${idx}`, "trigger"]);

  // --- Name (loc override on the rule's loc key) ---
  const pendingName = $derived(queue.pendingLocOverride(rule.name_key));
  const nameValue = $derived(pendingName ?? rule.name ?? rule.name_key);
  function commitName(v: string) {
    if (!rule.name_key) return;
    queue.push({
      label: `Edit colonial name`,
      edits: [{ kind: "locOverride", key: rule.name_key, value: v }],
      coalesceKey: `colname:${rule.name_key}`,
    });
  }

  // --- Trigger tree (lazy: parsed only while expanded) ---
  let expanded = $state(false);
  let triggerBlock = $state<ScriptBlock | null>(null);
  let parseError = $state<string | null>(null);
  let loadToken = 0;

  $effect(() => {
    // Track dependencies synchronously so re-parse fires on queue/selection change.
    void installPath;
    void modPath;
    void file;
    void expanded;
    void idx;
    queue.version;
    const token = ++loadToken;
    if (!expanded || !rule.has_trigger) {
      triggerBlock = null;
      return;
    }
    void reload(token);
  });

  async function reload(token: number) {
    parseError = null;
    try {
      const b = await invoke<ScriptBlock>("parse_script_block_with_edits", {
        installPath,
        modPath,
        file,
        path: triggerPath,
        edits: queue.serialize(),
      });
      if (token !== loadToken) return;
      triggerBlock = b;
    } catch (e) {
      if (token !== loadToken) return;
      triggerBlock = null;
      parseError = String(e);
    }
  }

  function onTreeEdit(edits: TypedEdit[], label: string) {
    if (edits.length) queue.push({ label, edits });
  }

  function addTrigger() {
    queue.push({
      label: `Add condition to colonial name`,
      edits: [
        {
          kind: "insertStatement",
          file,
          blockPath: [entryKey, `names#${idx}`],
          statement: `trigger = {\n}`,
        },
      ],
    });
    expanded = true;
  }
  function removeTrigger() {
    if (!confirm("Remove this rule's condition (it becomes an unconditional fallback name)?")) return;
    queue.push({
      label: `Remove condition from colonial name`,
      edits: [{ kind: "removeStatement", file, blockPath: [entryKey, `names#${idx}`], key: "trigger" }],
    });
    expanded = false;
  }
</script>

<div class="rule">
  <div class="rule-head">
    <span class="pos" title="Match order — first matching rule wins">#{idx + 1}</span>
    <input
      class="text name"
      value={nameValue}
      placeholder="(name loc)"
      oninput={(e) => commitName((e.target as HTMLInputElement).value)}
    />
    <div class="ord">
      <button class="ico" disabled={idx === 0} title="Move up (matched earlier)" aria-label="Move up" onclick={() => onmove(-1)}>▲</button>
      <button class="ico" disabled={idx === ruleCount - 1} title="Move down" aria-label="Move down" onclick={() => onmove(1)}>▼</button>
      <button class="ico danger" title="Delete rule" aria-label="Delete rule" onclick={onremove}>🗑</button>
    </div>
  </div>
  <div class="cond">
    {#if rule.has_trigger}
      <button class="cond-toggle" onclick={() => (expanded = !expanded)}>
        {expanded ? "▾" : "▸"} Condition
      </button>
      <button class="mini danger" title="Remove condition" onclick={removeTrigger}>remove condition</button>
    {:else}
      <span class="always">Always (unconditional fallback)</span>
      <button class="mini" onclick={addTrigger}>+ condition</button>
    {/if}
  </div>
  {#if expanded && rule.has_trigger}
    {#if parseError}
      <p class="err">{parseError}</p>
    {:else if triggerBlock}
      <ScriptTreeEditor
        {file}
        rootPath={triggerPath}
        block={triggerBlock}
        registry="triggers"
        known={triggers}
        {countries}
        onedit={onTreeEdit}
      />
    {:else}
      <p class="dim small">Loading condition…</p>
    {/if}
  {/if}
</div>

<style>
  .rule {
    border: 1px solid #232a33;
    background: #171b21;
    padding: 0.3rem 0.4rem;
    margin-bottom: 0.35rem;
  }
  .rule-head {
    display: flex;
    align-items: center;
    gap: 0.35rem;
  }
  .pos {
    font-size: 0.72rem;
    color: #9ca3af;
    width: 1.6rem;
    flex: none;
  }
  .text {
    background: #14181d;
    border: 1px solid #4b5563;
    color: #cfd4db;
    font-family: inherit;
    font-size: 0.82rem;
    padding: 0.15rem 0.35rem;
  }
  .name {
    flex: 1;
    min-width: 0;
  }
  .ord {
    display: flex;
    gap: 0.15rem;
  }
  .ico {
    border: 1px solid #4b5563;
    background: #2b323d;
    color: #cfd4db;
    font-size: 0.72rem;
    line-height: 1;
    padding: 0.15rem 0.3rem;
    cursor: pointer;
  }
  .ico:disabled {
    opacity: 0.35;
    cursor: default;
  }
  .ico:not(:disabled):hover {
    border-color: #4a6da7;
    background: #4a6da7;
    color: #fff;
  }
  .ico.danger {
    color: #fca5a5;
    border-color: #6b3630;
  }
  .ico.danger:hover {
    background: #7a2820;
    color: #fff;
  }
  .cond {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    margin-top: 0.25rem;
  }
  .cond-toggle {
    border: none;
    background: transparent;
    color: #9cc7ea;
    font-family: inherit;
    font-size: 0.76rem;
    cursor: pointer;
    padding: 0;
  }
  .always {
    font-size: 0.74rem;
    color: #8a919c;
    font-style: italic;
  }
  .mini {
    border: 1px solid #4b5563;
    background: #2b323d;
    color: #cfd4db;
    font-family: inherit;
    font-size: 0.7rem;
    padding: 0.05rem 0.35rem;
    cursor: pointer;
  }
  .mini:hover {
    border-color: #4a6da7;
    background: #4a6da7;
    color: #fff;
  }
  .mini.danger {
    color: #fca5a5;
    border-color: #6b3630;
  }
  .err {
    color: #fca5a5;
    font-size: 0.76rem;
    margin: 0.3rem 0 0;
  }
  .dim {
    color: #9ca3af;
  }
  .small {
    font-size: 0.74rem;
  }
</style>
