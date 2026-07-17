<!--
  HistoricalSection — historical setup (Sprint 1.2): the ordered historical idea
  groups (picked from common/ideas category groups) and historical units (from
  common/units). Both are ordered bare-token lists in the common/countries file;
  add / remove / reorder rewrite the whole list (setBlock when it exists, else an
  insert of the whole block), coalesced per list so a run of edits is one undo unit.
-->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { SearchDropdown } from "$lib/components/ui";
  import type { DropdownItem } from "$lib/components/ui";
  import type { EditQueue, TypedEdit } from "$lib/edits.svelte";
  import type { CountryDetails, IdeaGroupEntry } from "./types";

  let {
    installPath,
    modPath,
    tag,
    queue,
    details,
  }: {
    installPath: string;
    modPath: string | null;
    tag: string;
    queue: EditQueue;
    details: CountryDetails;
  } = $props();

  const file = $derived(details.country_file ?? `common/countries/${details.name}.txt`);

  let ideaGroups = $state<DropdownItem[]>([]);
  let units = $state<DropdownItem[]>([]);
  $effect(() => {
    invoke<IdeaGroupEntry[]>("list_idea_groups", { installPath, modPath })
      .then((g) => (ideaGroups = g.map((e) => ({ key: e.key, label: `${e.name} (${e.category})` }))))
      .catch(() => {});
    invoke<string[]>("list_units", { installPath, modPath })
      .then((u) => (units = u.map((k) => ({ key: k, label: k }))))
      .catch(() => {});
  });

  // Local ordered lists (init from base; reset on country change).
  let groupList = $state<string[]>([]);
  let unitList = $state<string[]>([]);
  const groupBase = $derived(details.historical_idea_groups);
  const unitBase = $derived(details.historical_units);
  $effect(() => {
    groupList = [...groupBase];
    unitList = [...unitBase];
  });

  function commit(list: string[], baseLen: number, key: string, label: string) {
    const value = list.join(" ");
    const edit: TypedEdit =
      baseLen > 0
        ? { kind: "setBlock", file, path: [key], value }
        : { kind: "insertStatement", file, blockPath: [], statement: `${key} = { ${value} }` };
    queue.push({ label, edits: [edit], coalesceKey: `hist:${tag}:${key}` });
  }

  function labelOf(items: DropdownItem[], key: string): string {
    return items.find((i) => i.key === key)?.label ?? key;
  }

  // --- Generic list ops for a given target ("group" | "unit") ---
  function opsFor(which: "group" | "unit") {
    const key = which === "group" ? "historical_idea_groups" : "historical_units";
    const label = which === "group" ? "historical idea groups" : "historical units";
    const baseLen = which === "group" ? groupBase.length : unitBase.length;
    const get = () => (which === "group" ? groupList : unitList);
    const set = (v: string[]) => (which === "group" ? (groupList = v) : (unitList = v));
    const push = () => commit(get(), baseLen, key, `Edit ${label} of ${tag}`);
    return {
      add(k: string) {
        if (!k || get().includes(k)) return;
        set([...get(), k]);
        push();
      },
      remove(k: string) {
        set(get().filter((x) => x !== k));
        push();
      },
      move(i: number, delta: number) {
        const l = [...get()];
        const j = i + delta;
        if (j < 0 || j >= l.length) return;
        [l[i], l[j]] = [l[j], l[i]];
        set(l);
        push();
      },
    };
  }
  const groupOps = opsFor("group");
  const unitOps = opsFor("unit");
</script>

<section>
  <h3>Historical Setup</h3>

  <div class="list-field">
    <div class="lbl">Historical Idea Groups <span class="count">({groupList.length})</span></div>
    {#each groupList as g, i (g)}
      <div class="ordered">
        <span class="pos">{i + 1}</span>
        <span class="name">{labelOf(ideaGroups, g)}</span>
        <button class="mv" onclick={() => groupOps.move(i, -1)} disabled={i === 0} aria-label="Up">↑</button>
        <button class="mv" onclick={() => groupOps.move(i, 1)} disabled={i === groupList.length - 1} aria-label="Down">↓</button>
        <button class="x" onclick={() => groupOps.remove(g)} aria-label="Remove">×</button>
      </div>
    {/each}
    <SearchDropdown items={ideaGroups} value={null} placeholder="Add idea group…" onselect={(k) => groupOps.add(k)} />
  </div>

  <div class="list-field">
    <div class="lbl">Historical Units <span class="count">({unitList.length})</span></div>
    {#each unitList as u, i (u)}
      <div class="ordered">
        <span class="pos">{i + 1}</span>
        <span class="name">{u}</span>
        <button class="mv" onclick={() => unitOps.move(i, -1)} disabled={i === 0} aria-label="Up">↑</button>
        <button class="mv" onclick={() => unitOps.move(i, 1)} disabled={i === unitList.length - 1} aria-label="Down">↓</button>
        <button class="x" onclick={() => unitOps.remove(u)} aria-label="Remove">×</button>
      </div>
    {/each}
    <SearchDropdown items={units} value={null} placeholder="Add unit…" onselect={(k) => unitOps.add(k)} />
  </div>
</section>

<style>
  section {
    margin-bottom: 1rem;
  }

  h3 {
    margin: 0 0 0.5rem;
    font-size: 0.8rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: #9ca3af;
  }

  .list-field {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    margin-bottom: 0.7rem;
  }

  .lbl {
    font-size: 0.72rem;
    text-transform: uppercase;
    letter-spacing: 0.03em;
    color: #8a919c;
  }

  .count {
    color: #6b7280;
  }

  .ordered {
    display: flex;
    align-items: center;
    gap: 0.3rem;
    font-size: 0.8rem;
  }

  .pos {
    width: 1.2rem;
    text-align: right;
    color: #6b7280;
    font-variant-numeric: tabular-nums;
  }

  .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .mv {
    border: 1px solid #1f242c;
    background: #3f4855;
    color: #cfd4db;
    font-size: 0.7rem;
    line-height: 1;
    padding: 0.1rem 0.3rem;
    cursor: pointer;
  }

  .mv:hover:not(:disabled) {
    background: #4a6da7;
    color: #fff;
  }

  .mv:disabled {
    opacity: 0.35;
    cursor: default;
  }

  .x {
    border: none;
    background: transparent;
    color: #9ca3af;
    cursor: pointer;
    font-size: 0.95rem;
    line-height: 1;
    padding: 0 0.2rem;
  }

  .x:hover {
    color: #fca5a5;
  }
</style>
