<script lang="ts">
  // Linked events for a disaster/incident: forward refs (on_start/on_end event
  // ids the object fires — jump into the Events overlay) plus the reverse scan
  // (Sprint 16 style — events that reference this object's key). Honest byte-level
  // scan; shows the files and occurrence counts.
  import { invoke } from "@tauri-apps/api/core";
  import type { EventRef, MechanicEventRef } from "$lib/mechanics";

  let {
    installPath,
    modPath,
    objKey,
    eventRefs,
    onopenevents,
  }: {
    installPath: string;
    modPath: string | null;
    objKey: string;
    eventRefs: EventRef[];
    onopenevents?: (id: string) => void;
  } = $props();

  let refs = $state<MechanicEventRef[] | null>(null);
  let loading = $state(false);
  let error = $state<string | null>(null);

  async function scan() {
    loading = true;
    error = null;
    try {
      refs = await invoke<MechanicEventRef[]>("find_mechanic_event_refs", {
        installPath,
        modPath,
        key: objKey,
      });
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }
</script>

<div class="links">
  {#if eventRefs.length > 0}
    <div class="fwd">
      <span class="lbl">Fires:</span>
      {#each eventRefs as er (er.key)}
        <button class="ev" title={`${er.key} = ${er.id}`} onclick={() => onopenevents?.(er.id)}>
          {er.key} → <code>{er.id}</code>
        </button>
      {/each}
    </div>
  {/if}

  {#if !refs && !loading}
    <button class="load" onclick={scan}>Find events referencing <code>{objKey}</code>…</button>
  {:else if loading}
    <p class="dim">Scanning events…</p>
  {:else if error}
    <p class="err">{error}</p>
  {:else if refs}
    {#if refs.length === 0}
      <p class="dim">No events reference <code>{objKey}</code>.</p>
    {:else}
      <div class="rev">
        <span class="lbl">Referenced by events ({refs.length}):</span>
        <div class="files">
          {#each refs as r (r.file)}
            <span class="file" title={`${r.count} occurrence(s) · ${r.origin}`}>
              {r.file.split("/").pop()}<span class="cnt">×{r.count}</span>
            </span>
          {/each}
        </div>
      </div>
    {/if}
  {/if}
</div>

<style>
  .links { margin-top: 0.3rem; font-size: 0.8rem; color: var(--text-1); display: flex; flex-direction: column; gap: 0.3rem; }
  .fwd { display: flex; align-items: center; gap: 0.3rem; flex-wrap: wrap; }
  .lbl { color: var(--text-2); font-size: 0.76rem; }
  .ev { border: 1px solid var(--bg-3); background: var(--bg-1); color: var(--text-1); font-family: inherit; font-size: 0.74rem; padding: 0.1rem 0.35rem; cursor: pointer; }
  .ev:hover { border-color: var(--accent); background: var(--bg-3); }
  .load { border: 1px solid var(--border-strong); background: var(--bg-2); color: var(--text-1); font-family: inherit; font-size: 0.76rem; padding: 0.2rem 0.5rem; cursor: pointer; align-self: flex-start; }
  .load:hover { border-color: var(--accent); background: var(--accent); color: var(--text-inverse); }
  code { color: var(--ok); background: var(--bg-0); padding: 0 0.25rem; font-size: 0.74rem; }
  .files { display: flex; flex-wrap: wrap; gap: 0.25rem; margin-top: 0.15rem; }
  .file { color: var(--text-1); background: var(--bg-0); padding: 0.05rem 0.3rem; font-size: 0.72rem; }
  .cnt { color: var(--text-2); margin-left: 0.2rem; }
  .dim { color: var(--text-2); }
  .err { color: var(--err); }
</style>
