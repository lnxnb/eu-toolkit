<!--
  WarRow — one war row in the Diplomacy tab's Wars section (Sprint 13.2). Shows
  the two lead belligerents' flags (first participant of each side), the war
  name, a side badge for the selected country, and "since <join date>". Clicking
  the row opens the War panel. Mirrors DiplomacyRow's chrome.
-->
<script lang="ts">
  import { getFlagUrl } from "$lib/flagCache";
  import type { DropdownItem } from "$lib/components/ui";
  import { formatDate, type Calendar } from "$lib/calendar";
  import type { War } from "./wars";
  import { ATTACKER, DEFENDER, sideOf } from "./wars";

  let {
    installPath,
    modPath,
    war,
    tag,
    countries,
    calendar = null,
    selected = false,
    onopen,
  }: {
    installPath: string;
    modPath: string | null;
    war: War;
    /** The country whose panel is open (for the side badge + join date). */
    tag: string;
    countries: DropdownItem[];
    calendar?: Calendar | null;
    selected?: boolean;
    onopen: () => void;
  } = $props();

  const leadAttacker = $derived(war.participants.find((p) => p.side === ATTACKER)?.tag ?? null);
  const leadDefender = $derived(war.participants.find((p) => p.side === DEFENDER)?.tag ?? null);
  const side = $derived(sideOf(war, tag));
  const myJoin = $derived(war.participants.find((p) => p.tag === tag)?.join_date ?? null);

  const warName = $derived(
    war.name ?? war.file.slice(war.file.lastIndexOf("/") + 1).replace(/\.txt$/i, ""),
  );

  function nameOf(t: string | null): string {
    if (!t) return "—";
    return countries.find((c) => c.key === t)?.label ?? t;
  }

  let attackerFlag = $state<string | null>(null);
  let defenderFlag = $state<string | null>(null);
  $effect(() => {
    let alive = true;
    if (leadAttacker) getFlagUrl(installPath, modPath, leadAttacker).then((u) => alive && (attackerFlag = u));
    else attackerFlag = null;
    return () => {
      alive = false;
    };
  });
  $effect(() => {
    let alive = true;
    if (leadDefender) getFlagUrl(installPath, modPath, leadDefender).then((u) => alive && (defenderFlag = u));
    else defenderFlag = null;
    return () => {
      alive = false;
    };
  });

  function showDate(d: string | null): string {
    if (!d) return "—";
    return calendar ? formatDate(d, calendar) : d;
  }
</script>

<button class="war" class:selected onclick={onopen} title="Open {warName}">
  <span class="belligerents">
    {#if attackerFlag}<img class="flag" src={attackerFlag} alt="" title={nameOf(leadAttacker)} />{:else}<span class="flag ph"></span>{/if}
    <span class="vs">vs</span>
    {#if defenderFlag}<img class="flag" src={defenderFlag} alt="" title={nameOf(leadDefender)} />{:else}<span class="flag ph"></span>{/if}
  </span>
  <span class="name">{warName}</span>
  {#if side === ATTACKER}
    <span class="badge attacker">attacker</span>
  {:else if side === DEFENDER}
    <span class="badge defender">defender</span>
  {/if}
  {#if !war.active_at_date}
    <span class="badge inactive">inactive</span>
  {/if}
  <span class="spacer"></span>
  {#if myJoin}
    <span class="since" title="Joined {myJoin}">since {showDate(myJoin)}</span>
  {/if}
</button>

<style>
  .war {
    display: flex;
    align-items: center;
    gap: 0.35rem;
    width: 100%;
    border: none;
    border-bottom: 1px solid #1f242c;
    background: transparent;
    color: #cfd4db;
    font-family: inherit;
    font-size: 0.82rem;
    padding: 0.2rem 0.15rem;
    cursor: pointer;
    text-align: left;
  }
  .war:last-child {
    border-bottom: none;
  }
  .war:hover {
    color: #fff;
  }
  .war.selected {
    background: rgba(74, 109, 167, 0.22);
  }
  .belligerents {
    display: inline-flex;
    align-items: center;
    gap: 0.2rem;
    flex: none;
  }
  .flag {
    width: 1.1rem;
    height: 1.1rem;
    object-fit: cover;
    border: 1px solid #1f242c;
    flex: none;
  }
  .flag.ph {
    display: inline-block;
    background: #3a4150;
  }
  .vs {
    font-size: 0.62rem;
    color: #8a919c;
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 8rem;
  }
  .spacer {
    flex: 1;
  }
  .since {
    font-size: 0.72rem;
    color: #9ca3af;
    white-space: nowrap;
  }
  .badge {
    font-size: 0.62rem;
    padding: 0.02rem 0.3rem;
    border: 1px solid rgba(0, 0, 0, 0.35);
    color: #fff;
    text-transform: uppercase;
    letter-spacing: 0.03em;
    flex: none;
  }
  .badge.attacker {
    background: #c0392b;
  }
  .badge.defender {
    background: #2f6da7;
  }
  .badge.inactive {
    background: #566;
  }
</style>
