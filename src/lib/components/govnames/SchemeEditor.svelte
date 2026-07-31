<!--
  SchemeEditor — Sprint 19.3, one expanded government-names scheme.

  A rank×role table (rank 1–3 rows × country-name/ruler/consort columns) whose
  cells show the RESOLVED loc string; editing a cell writes a loc override on
  that cell's loc key (the government_names file is never touched). Absent cells
  (a rank missing from a role block) render empty/disabled. Below it, the scheme's
  trigger via the 14.2 tree editor (lazy-parsed through
  parse_script_block_with_edits so it reflects pending edits), and the
  preserve-unknown raw keys (heir titles / custom) shown read-only.
-->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { ScriptTreeEditor } from "$lib/components/script";
  import type { KnownKey, ScriptBlock } from "$lib/components/script";
  import type { DropdownItem } from "$lib/components/ui";
  import type { EditQueue, TypedEdit } from "$lib/edits.svelte";
  import { ROLES, cellOf, type GovNameScheme } from "$lib/governmentNames";

  let {
    installPath,
    modPath,
    queue,
    scheme,
    triggers,
    countries = [],
    onremove,
  }: {
    installPath: string;
    modPath: string | null;
    queue: EditQueue;
    scheme: GovNameScheme;
    triggers: KnownKey[];
    countries?: DropdownItem[];
    onremove: () => void;
  } = $props();

  const file = $derived(scheme.file);
  const key = $derived(scheme.key);
  const triggerPath = $derived([key, "trigger"]);

  const ROLE_LABELS: Record<string, string> = {
    rank: "Country name",
    ruler_male: "Ruler ♂",
    ruler_female: "Ruler ♀",
    consort_male: "Consort ♂",
    consort_female: "Consort ♀",
  };
  const RANK_LABELS = ["Duchy", "Kingdom", "Empire"];

  // --- Cells (resolved string; pending loc override wins) ---
  function cellValue(role: string, rank: number): string {
    const c = cellOf(scheme, role, rank);
    if (!c) return "";
    return queue.pendingLocOverride(c.loc_key) ?? c.resolved;
  }
  function cellEdited(role: string, rank: number): boolean {
    const c = cellOf(scheme, role, rank);
    return !!c && queue.pendingLocOverride(c.loc_key) !== undefined;
  }
  function commitCell(role: string, rank: number, v: string) {
    const c = cellOf(scheme, role, rank);
    if (!c) return;
    queue.push({
      label: `Edit ${role} name of ${key}`,
      edits: [{ kind: "locOverride", key: c.loc_key, value: v }],
      coalesceKey: `gov:${c.loc_key}`,
    });
  }

  // --- Trigger tree (lazy: parsed only while shown) ---
  let triggerBlock = $state<ScriptBlock | null>(null);
  let parseError = $state<string | null>(null);
  let loadToken = 0;

  $effect(() => {
    void installPath;
    void modPath;
    void file;
    void key;
    queue.version;
    const token = ++loadToken;
    if (!scheme.has_trigger) {
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
      label: `Add condition to ${key}`,
      edits: [{ kind: "insertStatement", file, blockPath: [key], statement: `trigger = {\n}` }],
    });
  }
  function removeTrigger() {
    if (!confirm(`Remove the condition of "${key}" (it becomes an unconditional fallback)?`)) return;
    queue.push({
      label: `Remove condition from ${key}`,
      edits: [{ kind: "removeStatement", file, blockPath: [key], key: "trigger" }],
    });
  }
</script>

<div class="editor">
  <table class="grid">
    <thead>
      <tr>
        <th class="corner">Rank</th>
        {#each ROLES as role (role)}<th>{ROLE_LABELS[role]}</th>{/each}
      </tr>
    </thead>
    <tbody>
      {#each [1, 2, 3] as rank (rank)}
        <tr>
          <th class="rankh">{rank} · {RANK_LABELS[rank - 1]}</th>
          {#each ROLES as role (role)}
            {@const present = cellOf(scheme, role, rank) != null}
            <td>
              {#if present}
                <input
                  class="cell"
                  class:edited={cellEdited(role, rank)}
                  value={cellValue(role, rank)}
                  oninput={(e) => commitCell(role, rank, (e.target as HTMLInputElement).value)}
                />
              {:else}
                <span class="empty" title="This role has no entry for rank {rank}">—</span>
              {/if}
            </td>
          {/each}
        </tr>
      {/each}
    </tbody>
  </table>
  <p class="hint">
    Each cell edits the localized string for its loc key. Empty cells have no entry in the file for that rank.
  </p>

  <div class="cond-head">
    <span class="cond-title">Condition (trigger)</span>
    {#if scheme.has_trigger}
      <button class="mini danger" onclick={removeTrigger}>remove condition</button>
    {:else}
      <button class="mini" onclick={addTrigger}>＋ add condition</button>
    {/if}
  </div>
  {#if scheme.has_trigger}
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
  {:else}
    <p class="dim small">Always matches (unconditional). It should sit last, as a generic fallback.</p>
  {/if}

  {#if scheme.raw_extra.length > 0}
    <div class="raw-head">Advanced (read-only)</div>
    <p class="dim small">Unmodeled keys, preserved untouched on save.</p>
    <ul class="raw">
      {#each scheme.raw_extra as r (r)}<li><span class="mono">{r}</span></li>{/each}
    </ul>
  {/if}

  <div class="danger-zone">
    <button class="btn danger" onclick={onremove}>Delete scheme…</button>
  </div>
</div>

<style>
  .editor {
    padding: 0.35rem 0.1rem 0.2rem;
  }
  .grid {
    border-collapse: collapse;
    width: 100%;
    font-size: 0.8rem;
  }
  .grid th,
  .grid td {
    border: 1px solid var(--bg-1);
    padding: 0.15rem 0.25rem;
    text-align: left;
  }
  .grid thead th {
    background: var(--bg-2);
    color: var(--text-2);
    font-weight: 600;
    font-size: 0.72rem;
  }
  .corner {
    width: 6.5rem;
  }
  .rankh {
    background: var(--bg-1);
    color: var(--text-1);
    font-weight: 500;
    white-space: nowrap;
  }
  .cell {
    width: 100%;
    min-width: 5rem;
    box-sizing: border-box;
    background: var(--bg-0);
    border: 1px solid var(--border-strong);
    color: var(--text-1);
    font-family: inherit;
    font-size: 0.78rem;
    padding: 0.1rem 0.25rem;
  }
  .cell.edited {
    border-color: var(--warn);
    background: var(--bg-1);
  }
  .empty {
    color: var(--text-3);
  }
  .hint {
    margin: 0.3rem 0 0.5rem;
    font-size: 0.72rem;
    color: var(--text-2);
  }
  .cond-head {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    margin: 0.3rem 0 0.25rem;
  }
  .cond-title {
    font-size: 0.74rem;
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
  .raw-head {
    margin-top: 0.5rem;
    font-size: 0.74rem;
    text-transform: uppercase;
    letter-spacing: 0.03em;
    color: var(--text-2);
  }
  .raw {
    list-style: none;
    margin: 0.1rem 0 0;
    padding: 0;
    font-size: 0.78rem;
    color: var(--text-1);
  }
  .mono {
    font-family: ui-monospace, monospace;
    color: var(--text-2);
  }
  .danger-zone {
    margin-top: 0.6rem;
  }
  .btn {
    border: 1px solid var(--border-strong);
    background: transparent;
    color: var(--text-1);
    font-family: inherit;
    font-size: 0.78rem;
    padding: 0.2rem 0.6rem;
    cursor: pointer;
  }
  .btn.danger {
    color: var(--err);
    border-color: var(--danger-bg);
  }
  .btn.danger:hover {
    background: var(--danger-bg);
    border-color: var(--danger-bg);
    color: var(--text-inverse);
  }
  .err {
    color: var(--err);
    font-size: 0.76rem;
    margin: 0.3rem 0 0;
  }
  .dim {
    color: var(--text-2);
  }
  .small {
    font-size: 0.74rem;
  }
</style>
