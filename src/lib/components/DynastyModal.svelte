<!--
  DynastyModal — the "Choose Dynasty" modal (SPRINT 1.3), the prototype consumer
  of the shared ui/MultiSelectModal (search + checkbox multi-select + mass-action
  footer slot). Two modes over the same backend scan:

    • mode="pick"   — pick one dynasty for the field being edited, or type a
                      brand-new name ("a dynasty exists by being used"). The
                      chosen string is returned via onpick(name); the CALLER
                      makes the pending edit (this modal never touches the queue
                      in pick mode). Single-pick is "check one + Use", or the
                      "New dynasty…" free-text + "New" button.
    • mode="manage" — multi-select + MASS DELETE. Delete asks for confirmation
                      (listing the affected countries), then pushes ONE composite
                      of removeStatement edits (delete = remove the
                      `dynasty = "..."` line from every usage — a dynastyless
                      ruler is legal EU4, verified against vanilla: ~250 files
                      ship a dynastyless monarch, e.g. republics/tribes/hordes).

  ── Props contract ────────────────────────────────────────────────────────────
    open        (bindable) whether the modal is shown
    mode        'pick' | 'manage'
    onpick?     (name: string) => void — pick-mode callback (existing or new)
    queue?      EditQueue — required for manage-mode mass delete
    installPath string — session install (for the backend scan)
    modPath?    string | null — session mod (for the backend scan)
    dynasties?  DynastyEntry[] — TEST/BENCH injection: when provided, the modal
                uses it directly and skips the backend `scan_dynasties` invoke.

  Delete-edit shape: removeStatement { file, blockPath: usage.path, key:"dynasty",
  value: null }. value is null on purpose — the block-path already uniquely names
  the holder block (one dynasty per block), and a value filter would mis-match
  non-ASCII names (the backend stores Windows-1252 bytes, the JS string is UTF-8).
  Edits are deduped by (file, path) so the rare duplicate-date/holder collision
  (mod_writer addresses first-match) produces one edit rather than a failing
  double-remove — see dynasties.rs module docs for that limitation.
-->
<script lang="ts" module>
  /** One usage of a dynasty string (mirrors backend `dynasties::DynastyUsage`). */
  export interface DynastyUsage {
    tag: string;
    file: string;
    date: string | null;
    holder: "monarch" | "heir" | "queen";
    holderName: string | null;
    /** Block-path segments to the holder block: [date, holder] or [holder]. */
    path: string[];
  }

  /** A dynasty aggregated across game+mod (mirrors `dynasties::DynastyEntry`). */
  export interface DynastyEntry {
    name: string;
    count: number;
    usages: DynastyUsage[];
  }
</script>

<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { MultiSelectModal } from "$lib/components/ui";
  import type { MultiSelectItem } from "$lib/components/ui";
  import type { EditQueue, Composite, TypedEdit } from "$lib/edits.svelte";

  let {
    open = $bindable(false),
    mode,
    onpick,
    queue,
    installPath,
    modPath = null,
    dynasties,
  }: {
    open?: boolean;
    mode: "pick" | "manage";
    onpick?: (name: string) => void;
    queue?: EditQueue;
    installPath: string;
    modPath?: string | null;
    dynasties?: DynastyEntry[];
  } = $props();

  // Loaded dynasties (injected prop wins; else the backend scan).
  let loaded = $state<DynastyEntry[]>([]);
  let loading = $state(false);
  let error = $state<string | null>(null);

  // The list the UI shows: injected data if present, else the scan result.
  let entries = $derived(dynasties ?? loaded);
  let byName = $derived(new Map(entries.map((e) => [e.name, e])));

  let items = $derived<MultiSelectItem[]>(
    entries.map((e) => ({ key: e.name, label: e.name, badge: e.count })),
  );

  let selected = $state<string[]>([]);
  let newName = $state("");
  // When set, the delete confirmation overlay is shown for these dynasty names.
  let pendingDelete = $state<string[] | null>(null);

  // Load from the backend when opened without injected data. Re-runs if the
  // session (install/mod) changes while open.
  $effect(() => {
    if (!open || dynasties) return;
    void loadScan(installPath, modPath);
  });

  async function loadScan(install: string, mod: string | null) {
    loading = true;
    error = null;
    try {
      loaded = await invoke<DynastyEntry[]>("scan_dynasties", {
        installPath: install,
        modPath: mod,
      });
    } catch (e) {
      loaded = [];
      error = String(e);
    } finally {
      loading = false;
    }
  }

  // Reset transient state whenever the modal opens.
  $effect(() => {
    if (open) {
      selected = [];
      newName = "";
      pendingDelete = null;
    }
  });

  const title = $derived(mode === "pick" ? "Choose Dynasty" : "Manage Dynasties");
  const confirmLabel = $derived(mode === "pick" ? "Use selected" : "Close");

  function onConfirm(keys: string[]) {
    if (mode !== "pick") return;
    // Free-text takes priority; otherwise the most recently checked row.
    const chosen = newName.trim() || keys.at(-1);
    if (chosen) onpick?.(chosen);
  }

  function pickNew() {
    const n = newName.trim();
    if (!n) return;
    onpick?.(n);
    open = false;
  }

  // Affected country tags for the confirmation dialog, in stable order.
  let affectedTags = $derived.by(() => {
    if (!pendingDelete) return [];
    const tags = new Set<string>();
    for (const name of pendingDelete) {
      for (const u of byName.get(name)?.usages ?? []) tags.add(u.tag);
    }
    return [...tags].sort();
  });

  /** One composite of removeStatement edits deleting every usage of `names`. */
  function buildDeleteComposite(names: string[]): Composite {
    const edits: TypedEdit[] = [];
    const seen = new Set<string>();
    for (const name of names) {
      for (const u of byName.get(name)?.usages ?? []) {
        const dedupe = `${u.file}${u.path.join("")}`;
        if (seen.has(dedupe)) continue; // first-match addressing: one edit per block
        seen.add(dedupe);
        edits.push({
          kind: "removeStatement",
          file: u.file,
          blockPath: u.path,
          key: "dynasty",
          value: null,
        });
      }
    }
    const label =
      names.length === 1
        ? `Delete dynasty "${names[0]}"`
        : `Delete ${names.length} dynasties`;
    return { label, edits };
  }

  function confirmDelete() {
    if (!pendingDelete) return;
    queue?.push(buildDeleteComposite(pendingDelete));
    pendingDelete = null;
    selected = [];
    open = false;
  }
</script>

<MultiSelectModal
  bind:open
  {title}
  {items}
  bind:selected
  {confirmLabel}
  searchPlaceholder="Search dynasties…"
  onconfirm={onConfirm}
>
  {#snippet actions(sel)}
    {#if mode === "pick"}
      <div class="new-dynasty">
        <input
          type="text"
          placeholder="New dynasty…"
          bind:value={newName}
          onkeydown={(e) => {
            if (e.key === "Enter") {
              pickNew();
              e.preventDefault();
            }
          }}
        />
        <button class="dm-btn" disabled={newName.trim() === ""} onclick={pickNew}>
          + New
        </button>
      </div>
    {:else}
      <button
        class="dm-btn danger"
        disabled={sel.length === 0}
        onclick={() => (pendingDelete = [...sel])}
      >
        Delete{sel.length ? ` (${sel.length})` : ""}
      </button>
    {/if}
  {/snippet}
</MultiSelectModal>

<!-- Status line (loading / error), rendered as a fixed banner above the modal. -->
{#if open && (loading || error)}
  <div class="dm-status" class:err={!!error}>
    {#if loading}Scanning dynasties…{:else}{error}{/if}
  </div>
{/if}

<!-- Delete confirmation: sits above the modal (z 110). -->
{#if pendingDelete}
  <div class="confirm-root" role="dialog" aria-modal="true" aria-label="Confirm delete">
    <button class="confirm-backdrop" aria-label="Cancel" onclick={() => (pendingDelete = null)}
    ></button>
    <div class="confirm-box">
      <header>Delete {pendingDelete.length} dynast{pendingDelete.length === 1 ? "y" : "ies"}?</header>
      <div class="confirm-body">
        <p>
          This removes the <code>dynasty</code> line from every usage. Affected
          countries ({affectedTags.length}):
        </p>
        <div class="tags">
          {#each affectedTags as tag (tag)}<span class="tag">{tag}</span>{/each}
          {#if affectedTags.length === 0}<span class="muted">none</span>{/if}
        </div>
        <p class="muted small">
          Rulers keep their names; they simply become dynastyless (legal in EU4).
          Queued as one undoable edit — applied on Save.
        </p>
      </div>
      <footer>
        <button class="dm-btn" onclick={() => (pendingDelete = null)}>Cancel</button>
        <button class="dm-btn danger" onclick={confirmDelete}>Delete</button>
      </footer>
    </div>
  </div>
{/if}

<style>
  .new-dynasty {
    display: flex;
    gap: 0.3rem;
    align-items: center;
  }

  .new-dynasty input {
    width: 9rem;
    background: #21262e;
    border: 1px solid #1f242c;
    color: #cfd4db;
    font-family: inherit;
    font-size: 0.82rem;
    padding: 0.28rem 0.4rem;
    outline: none;
  }

  .dm-btn {
    border: 1px solid #1f242c;
    background: #3f4855;
    color: #cfd4db;
    font-family: inherit;
    font-size: 0.82rem;
    padding: 0.3rem 0.7rem;
    cursor: pointer;
  }

  .dm-btn:hover:not(:disabled) {
    background: #4a6da7;
    color: #ffffff;
  }

  .dm-btn:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .dm-btn.danger {
    background: #7a2e2e;
    color: #f2dede;
  }

  .dm-btn.danger:hover:not(:disabled) {
    background: #a13636;
    color: #ffffff;
  }

  .dm-status {
    position: fixed;
    top: 1rem;
    left: 50%;
    transform: translateX(-50%);
    z-index: 105;
    background: #3f4855;
    border: 1px solid #1f242c;
    color: #cfd4db;
    padding: 0.35rem 0.7rem;
    font-size: 0.8rem;
  }

  .dm-status.err {
    background: #7a2e2e;
    color: #f2dede;
    max-width: 32rem;
  }

  .confirm-root {
    position: fixed;
    inset: 0;
    z-index: 110;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .confirm-backdrop {
    position: absolute;
    inset: 0;
    border: none;
    background: rgba(0, 0, 0, 0.5);
    cursor: default;
  }

  .confirm-box {
    position: relative;
    z-index: 111;
    width: 24rem;
    max-width: calc(100vw - 2rem);
    background: #2b323d;
    border: 1px solid #1f242c;
    color: #cfd4db;
    box-shadow: 0 8px 28px rgba(0, 0, 0, 0.55);
  }

  .confirm-box header {
    padding: 0.5rem 0.7rem;
    background: #3f4855;
    border-bottom: 1px solid #1f242c;
    font-weight: 700;
  }

  .confirm-body {
    padding: 0.6rem 0.7rem;
    font-size: 0.85rem;
  }

  .confirm-body p {
    margin: 0 0 0.5rem;
  }

  .confirm-body code {
    background: #21262e;
    padding: 0 0.2rem;
  }

  .tags {
    display: flex;
    flex-wrap: wrap;
    gap: 0.3rem;
    max-height: 8rem;
    overflow-y: auto;
    margin-bottom: 0.5rem;
  }

  .tag {
    background: #21262e;
    border: 1px solid #1f242c;
    padding: 0.1rem 0.35rem;
    font-size: 0.78rem;
    font-variant-numeric: tabular-nums;
  }

  .muted {
    color: #8a919c;
  }

  .small {
    font-size: 0.76rem;
  }

  .confirm-box footer {
    display: flex;
    justify-content: flex-end;
    gap: 0.4rem;
    padding: 0.5rem 0.7rem;
    background: #262c35;
    border-top: 1px solid #1f242c;
  }
</style>
