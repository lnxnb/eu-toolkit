<script lang="ts">
  // Sprint 21 context: provinces whose folded revolt at the effective start date
  // is of this faction. Count + jump links (click → select the province on the
  // map, switch to Provinces mode). Scans history/provinces through the Vfs.
  import { invoke } from "@tauri-apps/api/core";
  import type { RebelProvince } from "$lib/rebels";

  let {
    installPath,
    modPath,
    date = null,
    faction,
    onopenprovince,
  }: {
    installPath: string;
    modPath: string | null;
    date?: string | null;
    faction: string;
    onopenprovince?: (id: number) => void;
  } = $props();

  let rows = $state<RebelProvince[]>([]);
  let loading = $state(false);
  let error = $state<string | null>(null);

  $effect(() => {
    const ip = installPath, mp = modPath, dt = date, fac = faction;
    loading = true;
    error = null;
    invoke<RebelProvince[]>("get_rebel_provinces", { installPath: ip, modPath: mp, date: dt, faction: fac })
      .then((r) => { rows = r; })
      .catch((e) => { error = String(e); })
      .finally(() => { loading = false; });
  });
</script>

{#if loading}
  <p class="dim small">Scanning province history…</p>
{:else if error}
  <p class="err small">{error}</p>
{:else}
  <p class="count">
    <strong>{rows.length}</strong>
    {rows.length === 1 ? "province has" : "provinces have"} a start-date revolt of this type.
  </p>
  {#if rows.length > 0}
    <div class="jumps">
      {#each rows as p (p.id)}
        <button class="jump" onclick={() => onopenprovince?.(p.id)} title={`Open #${p.id} on the map`}>
          {p.name} <span class="pid">#{p.id}</span>{#if p.date}<span class="pd">{p.date}</span>{/if}
        </button>
      {/each}
    </div>
  {/if}
{/if}

<style>
  .count { margin: 0.2rem 0; font-size: 0.8rem; color: var(--text-1); }
  .jumps { display: flex; flex-wrap: wrap; gap: 0.25rem; }
  .jump { display: inline-flex; align-items: center; gap: 0.3rem; border: 1px solid var(--border); background: var(--bg-1); color: var(--text-1); font-family: inherit; font-size: 0.76rem; padding: 0.08rem 0.4rem; cursor: pointer; }
  .jump:hover { background: var(--accent); color: var(--text-inverse); border-color: var(--accent); }
  .pid { color: var(--ok); font-size: 0.7rem; }
  .pd { color: var(--text-2); font-size: 0.68rem; }
  .dim { color: var(--text-2); }
  .err { color: var(--err); }
  .small { font-size: 0.74rem; }
</style>
