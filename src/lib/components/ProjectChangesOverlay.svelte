<!--
  ProjectChangesOverlay (View ▸ Project Changes) — Sprint 30.4.

  The whole mod vs vanilla: every project file classified as **added** (no base
  counterpart), **shadows** (overrides a base file), or the base folders **hidden**
  by a descriptor `replace_path`. Selecting a text file shows its line diff vs the
  base copy (added/removed lines); binary files show a size/type note.

  Complements the Edits panel (Sprint 30.1) — that is "this session, unsaved";
  this is "the whole mod on disk vs the base install".
-->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { OverlaySurface } from "$lib/components/script";

  interface FileClass { rel: string; class: "added" | "shadows"; binary: boolean; size: number }
  interface HiddenFolder { rel: string; baseFileCount: number }
  interface ProjectChanges { files: FileClass[]; hidden: HiddenFolder[] }
  interface DiffLine { tag: "same" | "add" | "del"; text: string }
  interface FileDiff { rel: string; added: boolean; binary: boolean; baseSize: number; modSize: number; lines: DiffLine[] }

  let {
    open = $bindable(false),
    installPath,
    modPath,
  }: {
    open?: boolean;
    installPath: string;
    modPath: string | null;
  } = $props();

  let data = $state<ProjectChanges | null>(null);
  let loading = $state(false);
  let selected = $state<string | null>(null);
  let diff = $state<FileDiff | null>(null);
  let diffLoading = $state(false);

  async function load() {
    loading = true;
    try {
      data = await invoke<ProjectChanges>("get_project_changes", { installPath, modPath });
    } catch {
      data = { files: [], hidden: [] };
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    void installPath;
    void modPath;
    if (open && !data && !loading) void load();
  });

  async function selectFile(rel: string) {
    selected = rel;
    diff = null;
    diffLoading = true;
    try {
      diff = await invoke<FileDiff>("get_file_diff", { installPath, modPath, rel });
    } catch {
      diff = null;
    } finally {
      diffLoading = false;
    }
  }

  // Group files by top-level folder for the tree, added-vs-shadows tallied.
  const tree = $derived.by(() => {
    const map = new Map<string, FileClass[]>();
    for (const f of data?.files ?? []) {
      const top = f.rel.split("/")[0] || "(root)";
      const arr = map.get(top) ?? [];
      arr.push(f);
      map.set(top, arr);
    }
    return [...map.entries()].sort((a, b) => a[0].localeCompare(b[0]));
  });

  const counts = $derived.by(() => {
    let added = 0, shadows = 0;
    for (const f of data?.files ?? []) f.class === "added" ? added++ : shadows++;
    return { added, shadows };
  });

  // Which top folders are expanded (all collapsed by default except small trees).
  let expanded = $state<Set<string>>(new Set());
  function toggle(top: string) {
    if (expanded.has(top)) expanded.delete(top);
    else expanded.add(top);
    expanded = new Set(expanded);
  }

  function fmtSize(n: number): string {
    if (n < 1024) return `${n} B`;
    if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
    return `${(n / (1024 * 1024)).toFixed(1)} MB`;
  }
</script>

<OverlaySurface bind:open title="Project Changes">
  {#snippet toolbar()}
    {#if data}
      <span class="tally"><span class="badge add">+{counts.added}</span> added</span>
      <span class="tally"><span class="badge shadow">±{counts.shadows}</span> shadowed</span>
      {#if data.hidden.length}<span class="tally"><span class="badge hide">⊘{data.hidden.length}</span> hidden</span>{/if}
    {/if}
    <button class="rerun" disabled={loading} onclick={() => { data = null; void load(); }}>
      {loading ? "Scanning…" : "Rescan"}
    </button>
  {/snippet}

  <div class="split">
    <div class="tree">
      {#if modPath === null}
        <p class="hint">The base game has no project changes. Open a mod project to compare against vanilla.</p>
      {:else if loading && !data}
        <p class="hint">Scanning project…</p>
      {:else if data}
        {#if data.hidden.length}
          <section class="hidden-sec">
            <header class="grp-head static"><span class="grp-name">Hidden by replace_path</span></header>
            <ul class="flist">
              {#each data.hidden as h (h.rel)}
                <li class="hidden-row" title="Base folder masked entirely by the mod">
                  <span class="badge hide">⊘</span>
                  <span class="hrel">{h.rel}/</span>
                  <span class="hcount">{h.baseFileCount} base file{h.baseFileCount === 1 ? "" : "s"} hidden</span>
                </li>
              {/each}
            </ul>
          </section>
        {/if}
        {#each tree as [top, files] (top)}
          <section class="grp">
            <button class="grp-head" onclick={() => toggle(top)}>
              <span class="chev">{expanded.has(top) ? "▾" : "▸"}</span>
              <span class="grp-name">{top}/</span>
              <span class="grp-count">{files.length}</span>
            </button>
            {#if expanded.has(top)}
              <ul class="flist">
                {#each files as f (f.rel)}
                  <li>
                    <button class="frow" class:sel={selected === f.rel} onclick={() => selectFile(f.rel)}>
                      <span class="badge" class:add={f.class === "added"} class:shadow={f.class === "shadows"}>
                        {f.class === "added" ? "+" : "±"}
                      </span>
                      <span class="frel" title={f.rel}>{f.rel.slice(top.length + 1)}</span>
                      {#if f.binary}<span class="bin">bin</span>{/if}
                    </button>
                  </li>
                {/each}
              </ul>
            {/if}
          </section>
        {/each}
        {#if data.files.length === 0 && data.hidden.length === 0}
          <p class="hint">No project files differ from the base install.</p>
        {/if}
      {/if}
    </div>

    <div class="diff">
      {#if !selected}
        <p class="hint">Select a file to see its diff against the base install.</p>
      {:else if diffLoading}
        <p class="hint">Diffing {selected}…</p>
      {:else if diff}
        <header class="diff-head">
          <span class="dtitle" title={diff.rel}>{diff.rel}</span>
          <span class="dmeta">
            {#if diff.added}new file · {fmtSize(diff.modSize)}
            {:else}base {fmtSize(diff.baseSize)} → mod {fmtSize(diff.modSize)}{/if}
          </span>
        </header>
        {#if diff.binary}
          <p class="hint">Binary asset — {fmtSize(diff.modSize)}{#if !diff.added} (base {fmtSize(diff.baseSize)}){/if}. No text diff.</p>
        {:else if diff.added}
          <p class="hint">Added file — no base counterpart to diff against. Use Find in Project or open in an editor to view its contents.</p>
        {:else}
          <pre class="diff-pre">{#each diff.lines as l}<div class="dl {l.tag}"><span class="sign">{l.tag === "add" ? "+" : l.tag === "del" ? "-" : " "}</span>{l.text}
</div>{/each}</pre>
        {/if}
      {:else}
        <p class="hint">Could not diff this file.</p>
      {/if}
    </div>
  </div>
</OverlaySurface>

<style>
  .tally { font-size: 0.74rem; color: var(--text-2); display: inline-flex; align-items: center; gap: 0.3rem; }
  .rerun { border: 1px solid var(--accent); background: var(--accent); color: var(--text-inverse); font-family: inherit; font-size: 0.74rem; padding: 0.16rem 0.6rem; cursor: pointer; margin-left: auto; }
  .rerun:disabled { opacity: 0.6; cursor: default; }

  .split { display: flex; gap: 0.6rem; height: 100%; min-height: 0; }
  .tree { flex: 0 0 34%; max-width: 34%; overflow: auto; border: 1px solid var(--border); background: var(--bg-2); }
  .diff { flex: 1; min-width: 0; overflow: auto; border: 1px solid var(--border); background: var(--bg-0); }

  .hint { color: var(--text-2); font-size: 0.82rem; padding: 0.7rem; }

  .grp, .hidden-sec { border-bottom: 1px solid var(--border); }
  .grp-head { display: flex; align-items: center; gap: 0.4rem; width: 100%; text-align: left; border: none; background: var(--bg-1); color: var(--text-1); font: inherit; font-size: 0.8rem; padding: 0.28rem 0.5rem; cursor: pointer; }
  .grp-head.static { cursor: default; }
  .grp-head:not(.static):hover { background: var(--bg-3); }
  .chev { width: 0.8rem; color: var(--text-2); }
  .grp-name { flex: 1; font-weight: 600; }
  .grp-count { font-size: 0.7rem; color: var(--text-2); font-variant-numeric: tabular-nums; }

  .flist { list-style: none; margin: 0; padding: 0; }
  .frow { display: flex; align-items: center; gap: 0.4rem; width: 100%; text-align: left; border: none; border-bottom: 1px solid var(--bg-1); background: transparent; color: var(--text-1); font: inherit; font-size: 0.76rem; padding: 0.14rem 0.5rem 0.14rem 1.3rem; cursor: pointer; }
  .frow:hover { background: var(--bg-3); }
  .frow.sel { background: var(--accent); color: var(--text-inverse); }
  .frel { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .bin { font-size: 0.62rem; color: var(--text-2); border: 1px solid var(--bg-3); padding: 0 0.2rem; }

  .hidden-row { display: flex; align-items: center; gap: 0.4rem; padding: 0.14rem 0.5rem; font-size: 0.74rem; color: var(--text-1); }
  .hrel { color: var(--warn); }
  .hcount { color: var(--text-2); font-size: 0.7rem; }

  .badge { display: inline-flex; align-items: center; justify-content: center; min-width: 1rem; height: 1rem; font-size: 0.66rem; font-weight: 700; color: var(--text-inverse); border-radius: 1px; padding: 0 0.15rem; }
  .badge.add { background: var(--ok); }
  .badge.shadow { background: var(--accent); }
  .badge.hide { background: var(--warn); color: var(--bg-0); }

  .diff-head { display: flex; align-items: baseline; gap: 0.6rem; padding: 0.4rem 0.6rem; background: var(--bg-1); border-bottom: 1px solid var(--border); position: sticky; top: 0; }
  .dtitle { font-weight: 700; font-size: 0.82rem; color: var(--ok); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .dmeta { font-size: 0.72rem; color: var(--text-2); white-space: nowrap; }

  .diff-pre { margin: 0; font-family: "Cascadia Code", "Consolas", monospace; font-size: 0.75rem; color: var(--text-1); }
  .dl { display: block; white-space: pre; padding: 0 0.5rem; }
  .dl .sign { display: inline-block; width: 1rem; color: var(--text-3); user-select: none; }
  .dl.add { background: var(--bg-1); }
  .dl.add .sign { color: var(--ok); }
  .dl.del { background: var(--danger-bg); }
  .dl.del .sign { color: var(--err); }
</style>
