<!--
  EmperorTimelineSection — the dated emperor successions for one empire (HRE or
  Mandate), Sprint 29. Reuses the Timeline edit RECIPE (empires.ts emperor*Edits
  → the same byte-surgical dated-block machinery as country history) with an
  emperor-specific compact view: "current emperor at the selected date" header +
  per-succession row (date, tag, validation badges) + add/edit/remove. Pending
  edits fold live via foldEmperorTimeline.
-->
<script lang="ts">
  import { untrack } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { DatePicker, SearchDropdown } from "$lib/components/ui";
  import type { DropdownItem } from "$lib/components/ui";
  import type { EditQueue } from "$lib/edits.svelte";
  import {
    foldEmperorTimeline,
    emperorAddEdits,
    emperorEditEdits,
    emperorRemoveEdits,
    type EmperorTimeline,
    type EmperorEntry,
  } from "$lib/empires";

  let {
    installPath,
    modPath,
    kind,
    label,
    queue,
    countries,
    date = null,
  }: {
    installPath: string;
    modPath: string | null;
    kind: "hre" | "celestial";
    label: string;
    queue: EditQueue;
    countries: DropdownItem[];
    date?: string | null;
  } = $props();

  let fetched = $state<EmperorTimeline | null>(null);
  let error = $state<string | null>(null);
  let newDate = $state(untrack(() => date) ?? "1444.11.11");
  let newTag = $state<string | null>(null);

  $effect(() => {
    void load(installPath, modPath, kind, date);
  });
  async function load(install: string, mod: string | null, k: string, d: string | null) {
    try {
      fetched = await invoke<EmperorTimeline>("get_emperor_timeline", {
        installPath: install,
        modPath: mod,
        kind: k,
        date: d,
      });
      error = null;
    } catch (e) {
      error = String(e);
      fetched = null;
    }
  }

  const selDate = $derived(date ?? fetched?.date ?? "1444.11.11");
  const tl = $derived<EmperorTimeline | null>(
    fetched ? ((queue.version, foldEmperorTimeline(fetched, queue.serialize(), selDate))) : null,
  );

  function nameOf(tag: string): string {
    if (tag.replace(/-/g, "").length === 0) return "— none —";
    return countries.find((c) => c.key === tag.toUpperCase())?.label ?? tag;
  }

  function addSuccession() {
    if (!tl || !newTag) return;
    queue.push({
      label: `Add ${label} emperor ${newTag} @ ${newDate}`,
      edits: emperorAddEdits(tl, newDate, newTag),
      date: newDate,
    });
    newTag = null;
  }
  function editSuccession(e: EmperorEntry, tag: string) {
    if (!tl || tag === e.tag) return;
    queue.push({ label: `Set ${label} emperor ${tag} @ ${e.date}`, edits: emperorEditEdits(tl, e, tag), date: e.date });
  }
  function removeSuccession(e: EmperorEntry) {
    if (!tl) return;
    queue.push({ label: `Remove ${label} emperor @ ${e.date}`, edits: emperorRemoveEdits(tl, e), date: e.date });
  }

  const tagItems = $derived<DropdownItem[]>([{ key: "---", label: "— none (---) —" }, ...countries]);
</script>

<div class="etl">
  {#if error}
    <p class="err">{error}</p>
  {:else if tl}
    <div class="cur">
      <span class="curlabel">Emperor at {selDate}:</span>
      {#if tl.current}
        <strong>{tl.currentName ?? tl.current}</strong> <code>{tl.current}</code>
      {:else}
        <em>none</em>
      {/if}
    </div>

    <ul class="rows">
      {#each tl.entries as e (e.file + "::" + e.date + "#" + e.occurrenceIndex)}
        <li class="row" class:future={e.postSelected}>
          <span class="date">{e.date}</span>
          <div class="tagsel">
            <SearchDropdown
              items={tagItems}
              value={e.tag}
              placeholder="tag"
              onselect={(k) => editSuccession(e, k)}
            />
          </div>
          <span class="cname">{nameOf(e.tag)}</span>
          {#if e.postSelected}<span class="badge future">future</span>{/if}
          {#if !e.validTag}<span class="badge warn" title="No country with this tag exists">unknown tag</span>{/if}
          {#if e.isSubject}<span class="badge warn" title="This emperor is a subject at this date">subject</span>{/if}
          <button class="del" title="Remove succession" onclick={() => removeSuccession(e)}>✕</button>
        </li>
      {/each}
      {#if tl.entries.length === 0}
        <li class="empty">No emperor successions defined.</li>
      {/if}
    </ul>

    <div class="addrow">
      <DatePicker bind:value={newDate} />
      <div class="tagsel">
        <SearchDropdown items={tagItems} bind:value={newTag} placeholder="new emperor tag…" />
      </div>
      <button class="add" disabled={!newTag} onclick={addSuccession}>＋ Add succession</button>
    </div>
    <p class="note">New successions are written to <code>{tl.writeFile.split("/").pop()}</code>{tl.writeFileExists ? "" : " (created on save)"}.</p>
  {/if}
</div>

<style>
  .etl { display: flex; flex-direction: column; gap: 0.5rem; }
  .cur { font-size: 0.9rem; color: #cfd4db; padding: 0.2rem 0; }
  .curlabel { color: #8a919c; }
  .cur code { color: #9aecc0; background: #16191f; padding: 0 0.3rem; font-size: 0.78rem; }
  .rows { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; }
  .row { display: flex; align-items: center; gap: 0.5rem; padding: 0.28rem 0.3rem; border-bottom: 1px solid #1f242c; }
  .row.future { opacity: 0.7; }
  .date { font-family: monospace; font-size: 0.8rem; color: #c9a978; width: 6.5rem; flex: none; }
  .tagsel { width: 14rem; }
  .cname { font-size: 0.84rem; color: #cfd4db; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 12rem; }
  .badge { font-size: 0.66rem; text-transform: uppercase; letter-spacing: 0.03em; padding: 0.05rem 0.35rem; border: 1px solid #1f242c; }
  .badge.future { background: #2b323d; color: #8a919c; }
  .badge.warn { background: #5a2b2b; color: #f0c9c2; }
  .del { margin-left: auto; border: 1px solid #1f242c; background: #3f4855; color: #cfd4db; cursor: pointer; font-size: 0.75rem; padding: 0.1rem 0.4rem; }
  .del:hover { background: #7a3b3b; color: #fff; }
  .empty { color: #8a919c; font-size: 0.85rem; padding: 0.3rem 0; }
  .addrow { display: flex; align-items: center; gap: 0.5rem; flex-wrap: wrap; padding-top: 0.3rem; }
  .add { border: 1px solid #1f242c; background: #3f4855; color: #cfd4db; font-family: inherit; font-size: 0.82rem; padding: 0.28rem 0.7rem; cursor: pointer; }
  .add:hover:not(:disabled) { background: #4a6da7; color: #fff; }
  .add:disabled { opacity: 0.5; cursor: default; }
  .note { font-size: 0.76rem; color: #6d7683; margin: 0.1rem 0 0; }
  .note code { color: #9aecc0; }
  .err { color: #d9756b; font-size: 0.85rem; }
</style>
