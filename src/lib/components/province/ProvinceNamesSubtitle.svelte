<script lang="ts">
  // The province header's subtitle (Sprint 24 reverse view, promoted out of the
  // body): every culture / culture group / country tag that renames THIS
  // province, with the name it assigns. Culture rows jump to the culture
  // (cultures mode + select); tag rows jump to the country (political mode +
  // select); a culture GROUP is not a selectable map entity, so it is plain
  // text. Read-only — editing happens from the owning entity's panel.
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

  const COLLAPSED = 3;

  let rows = $state<ProvinceNameAssignment[]>([]);
  let expanded = $state(false);

  $effect(() => {
    const pid = id;
    rows = [];
    expanded = false;
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

  let shown = $derived(expanded ? rows : rows.slice(0, COLLAPSED));
  let hidden = $derived(rows.length - shown.length);

  function jump(a: ProvinceNameAssignment) {
    if (a.kind === "tag") onjumpcountry?.(a.key);
    else if (a.kind === "culture") onjumpculture?.(a.key);
  }

  /** The per-source capital name has no room inline, so it rides the tooltip. */
  function hint(a: ProvinceNameAssignment): string {
    const via = a.kind === "tag" ? "country" : a.kind === "group" ? "culture group" : "culture";
    const cap = a.capital ? ` · capital: ${a.capital}` : "";
    return `${a.label} (${via}): ${a.name}${cap}`;
  }
</script>

{#if rows.length > 0}
  <p class="names">
    {#each shown as a, i (a.kind + a.key)}
      {#if i > 0}<span class="sep">·</span>{/if}
      {#if a.kind === "group"}
        <span class="entry" title={hint(a)}>
          <span class="src">{a.label}</span>{a.name}
        </span>
      {:else}
        <button class="entry link" title={hint(a)} onclick={() => jump(a)}>
          <span class="src">{a.label}</span>{a.name}
        </button>
      {/if}
    {/each}
    {#if hidden > 0}
      <button class="more" onclick={() => (expanded = true)}>+{hidden} more</button>
    {:else if expanded && rows.length > COLLAPSED}
      <button class="more" onclick={() => (expanded = false)}>less</button>
    {/if}
  </p>
{/if}

<style>
  .names {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 0 var(--sp-1);
    margin: 0;
    font-size: var(--fs-xs);
    color: var(--text-2);
  }
  .sep { color: var(--text-3); }
  .entry {
    padding: 0;
    border: 0;
    background: transparent;
    font: inherit;
    color: var(--text-2);
  }
  .src {
    margin-right: 0.3em;
    color: var(--text-3);
  }
  .link { cursor: pointer; }
  .link:hover { color: var(--accent-text); }
  .link:hover .src { color: var(--accent-text); }
  .more {
    padding: 0;
    border: 0;
    background: transparent;
    font: inherit;
    color: var(--accent-text);
    cursor: pointer;
  }
</style>
