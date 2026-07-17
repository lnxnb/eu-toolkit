<script lang="ts">
  // Reverse view (Sprint 24): every culture / culture group / country tag that
  // renames THIS province, with the name each assigns. Culture rows jump to the
  // culture (cultures mode + select); tag rows jump to the country (political
  // mode + select). Read-only — editing happens from the owning entity's panel.
  import { invoke } from "@tauri-apps/api/core";
  import type { ProvinceNameAssignment } from "$lib/provinceNames";

  let {
    installPath,
    modPath,
    id,
    onjumpculture,
    onjumpcountry,
  }: {
    installPath: string;
    modPath: string | null;
    id: number;
    onjumpculture?: (key: string) => void;
    onjumpcountry?: (tag: string) => void;
  } = $props();

  let rows = $state<ProvinceNameAssignment[]>([]);
  let expanded = $state(false);

  $effect(() => {
    const pid = id;
    rows = [];
    invoke<ProvinceNameAssignment[]>("get_province_name_assignments", {
      installPath,
      modPath,
      id: pid,
    })
      .then((r) => {
        if (pid === id) rows = r;
      })
      .catch(() => {});
  });

  function jump(a: ProvinceNameAssignment) {
    if (a.kind === "tag") onjumpcountry?.(a.key);
    else if (a.kind === "culture") onjumpculture?.(a.key);
  }
</script>

{#if rows.length > 0}
  <section>
    <h3>
      <button class="hdr" onclick={() => (expanded = !expanded)}>
        Renamed by {rows.length}
        {rows.length === 1 ? "source" : "sources"}
        <span class="chev">{expanded ? "▾" : "▸"}</span>
      </button>
    </h3>
    {#if expanded}
      <ul class="rows">
        {#each rows as a (a.kind + a.key)}
          <li>
            <span class="kind {a.kind}">{a.kind}</span>
            {#if a.kind === "group"}
              <span class="key">{a.label}</span>
            {:else}
              <button class="key link" onclick={() => jump(a)} title="Jump to {a.key}">
                {a.label} ↗
              </button>
            {/if}
            <span class="nm">{a.name}</span>
            {#if a.capital}<span class="cap" title="Capital city name">⌂ {a.capital}</span>{/if}
          </li>
        {/each}
      </ul>
    {/if}
  </section>
{/if}

<style>
  section {
    padding: 0.4rem 0 0.6rem;
    border-bottom: 1px solid #232a33;
  }
  h3 {
    margin: 0;
    font-size: 0.8rem;
  }
  .hdr {
    background: none;
    border: none;
    color: #9ca3af;
    font-family: inherit;
    font-size: 0.8rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    cursor: pointer;
    padding: 0;
    display: flex;
    align-items: center;
    gap: 0.4rem;
  }
  .chev {
    color: #6b7280;
  }
  .rows {
    list-style: none;
    margin: 0.3rem 0 0;
    padding: 0;
  }
  .rows li {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    padding: 0.12rem 0;
    font-size: 0.8rem;
  }
  .kind {
    font-size: 0.66rem;
    text-transform: uppercase;
    letter-spacing: 0.03em;
    padding: 0 0.3rem;
    border-radius: 2px;
    border: 1px solid #2b323d;
    color: #9ca3af;
    background: #1a1f27;
  }
  .kind.culture {
    color: #c4b5fd;
  }
  .kind.group {
    color: #93c5fd;
  }
  .kind.tag {
    color: #fca5a5;
  }
  .key {
    color: #cfd4db;
    min-width: 5rem;
  }
  .link {
    background: none;
    border: none;
    color: #9cc7ea;
    font-family: inherit;
    font-size: 0.8rem;
    cursor: pointer;
    padding: 0;
    text-align: left;
  }
  .link:hover {
    color: #ffffff;
  }
  .nm {
    flex: 1;
    color: #e5e7eb;
  }
  .cap {
    color: #9cc7ea;
    font-size: 0.74rem;
  }
</style>
