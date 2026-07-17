<script lang="ts">
  // Country-panel Estates section (Sprint 20): per-estate starting-privileges
  // editor. Reads the country history file's start-date estate state
  // (`set_estate_privilege = <priv>`, folded to the selected date), grouped by the
  // owning estate. Add/remove privileges per estate via pickers filtered to that
  // estate's privileges. Sprint 12 date-aware: adds route through editAtDate
  // (top-level when selectedDate ≤ start, else a dated block).
  import { invoke } from "@tauri-apps/api/core";
  import { compareDates } from "$lib/calendar";
  import { editAtDate } from "$lib/editAtDate";
  import type { EditQueue, TypedEdit } from "$lib/edits.svelte";
  import type { CountryEstates, EstateBrief, StartingPrivilege } from "$lib/estates";
  import { SET_PRIVILEGE, foldStartingPrivileges } from "./starting";

  let {
    installPath,
    modPath,
    tag,
    queue,
    date = null,
    startDate = "1444.11.11",
    onopenestates,
  }: {
    installPath: string;
    modPath: string | null;
    tag: string;
    queue: EditQueue;
    date?: string | null;
    startDate?: string;
    onopenestates?: (key?: string) => void;
  } = $props();

  let fetched = $state<CountryEstates | null>(null);
  let error = $state<string | null>(null);
  let open = $state(true);
  let addFor = $state<string | null>(null); // estate key whose picker is open

  $effect(() => {
    void tag;
    void date;
    void load();
  });

  async function load() {
    error = null;
    try {
      fetched = await invoke<CountryEstates>("get_country_estates", {
        installPath,
        modPath,
        date,
        tag,
      });
    } catch (e) {
      error = String(e);
      fetched = null;
    }
  }

  const file = $derived(fetched?.file ?? `history/countries/${tag} - .txt`);
  const estates = $derived<EstateBrief[]>(fetched?.estates ?? []);

  // Effective starting privileges = backend (disk, date-folded) + pending edits
  // visible at the selected date.
  const effective = $derived.by<StartingPrivilege[]>(() => {
    if (!fetched) return [];
    queue.version;
    const visible = queue.serializeVisibleAt(
      (d) => d == null || date == null || compareDates(d, date) <= 0,
    );
    return foldStartingPrivileges(fetched, visible);
  });

  // Group by estate (privileges with an unknown estate fall under "Other").
  interface Group {
    estate: EstateBrief | null;
    key: string;
    granted: StartingPrivilege[];
  }
  const groups = $derived.by<Group[]>(() => {
    const out: Group[] = [];
    for (const est of estates) {
      out.push({
        estate: est,
        key: est.key,
        granted: effective.filter((s) => s.estate === est.key),
      });
    }
    const orphans = effective.filter((s) => !s.estate || !estates.some((e) => e.key === s.estate));
    if (orphans.length) out.push({ estate: null, key: "__other__", granted: orphans });
    return out.filter((g) => g.granted.length > 0 || g.estate != null);
  });

  function availableFor(est: EstateBrief): { key: string; name: string }[] {
    const held = new Set(effective.map((s) => s.privilege));
    return est.privileges.filter((p) => !held.has(p.key));
  }

  function addPrivilege(priv: string) {
    const startEdits: TypedEdit[] = [
      { kind: "insertStatement", file, blockPath: [], statement: `${SET_PRIVILEGE} = ${priv}` },
    ];
    const edits = editAtDate({
      file,
      selectedDate: date,
      startDate,
      datedBlocks: [],
      startEdits,
      statements: [`${SET_PRIVILEGE} = ${priv}`],
    });
    const post = date != null && compareDates(date, startDate) > 0;
    queue.push({
      label: `Grant ${priv} to ${tag}`,
      edits,
      ...(post ? { date } : {}),
    });
    addFor = null;
  }

  function removePrivilege(s: StartingPrivilege) {
    // A pending-added grant: drop it from the queue instead of writing a removal.
    const pendingAdd = queue.findLast(
      (e) =>
        (e.kind === "insertStatement" &&
          e.file === file &&
          e.statement.includes(`${SET_PRIVILEGE} = ${s.privilege}`)) ||
        (e.kind === "insertDatedBlock" &&
          e.file === file &&
          e.statement.includes(`${SET_PRIVILEGE} = ${s.privilege}`)),
    );
    if (pendingAdd) {
      queue.removeWhere((c) =>
        c.edits.some(
          (e) =>
            (e.kind === "insertStatement" || e.kind === "insertDatedBlock") &&
            e.file === file &&
            e.statement.includes(`${SET_PRIVILEGE} = ${s.privilege}`),
        ),
      );
      return;
    }
    // A disk grant: emit a removal at its block (top-level or its dated block).
    const blockPath = s.date ? [s.date] : [];
    const post = date != null && compareDates(date, startDate) > 0;
    queue.push({
      label: `Revoke ${s.privilege} from ${tag}`,
      edits: [{ kind: "removeStatement", file, blockPath, key: SET_PRIVILEGE, value: s.privilege }],
      ...(post && s.date ? { date: s.date } : {}),
    });
  }
</script>

<section class="estates">
  <button class="head" onclick={() => (open = !open)}>
    <span class="caret">{open ? "▾" : "▸"}</span>
    <span class="h-title">Estates</span>
    <span class="count">{effective.length} starting {effective.length === 1 ? "privilege" : "privileges"}</span>
  </button>

  {#if open}
    {#if error}
      <p class="err">{error}</p>
    {:else if !fetched}
      <p class="dim">Loading…</p>
    {:else}
      {#each groups as g (g.key)}
        <div class="grp">
          <div class="grp-head">
            <strong>{g.estate ? g.estate.name : "Other / unknown estate"}</strong>
            {#if g.estate}
              <button class="jump" title="Open in Estates editor" onclick={() => onopenestates?.(g.estate!.key)}>edit</button>
            {/if}
          </div>
          {#if g.granted.length === 0}
            <p class="dim small">No starting privileges.</p>
          {:else}
            <div class="chips">
              {#each g.granted as s (s.privilege)}
                <span class="chip" title={s.date ? `granted ${s.date}` : "granted at start"}>
                  {s.name}
                  {#if s.date}<span class="cdate">·{s.date}</span>{/if}
                  <button class="x" aria-label="Revoke" onclick={() => removePrivilege(s)}>×</button>
                </span>
              {/each}
            </div>
          {/if}
          {#if g.estate}
            {@const avail = availableFor(g.estate)}
            {#if addFor === g.estate.key}
              <select
                class="picker"
                onchange={(e) => {
                  const v = (e.target as HTMLSelectElement).value;
                  if (v) addPrivilege(v);
                }}
              >
                <option value="">— pick a privilege —</option>
                {#each avail as p (p.key)}<option value={p.key}>{p.name} ({p.key})</option>{/each}
              </select>
              <button class="mini" onclick={() => (addFor = null)}>cancel</button>
            {:else if avail.length > 0}
              <button class="mini" onclick={() => (addFor = g.estate!.key)}>＋ grant privilege…</button>
            {/if}
          {/if}
        </div>
      {/each}
    {/if}
  {/if}
</section>

<style>
  .estates {
    border-top: 1px solid #232a33;
    padding: 0.35rem 0 0.2rem;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    width: 100%;
    border: none;
    background: transparent;
    color: #cfd4db;
    font-family: inherit;
    cursor: pointer;
    padding: 0.1rem 0;
  }
  .caret {
    color: #8a919c;
    width: 0.8rem;
  }
  .h-title {
    font-weight: 600;
    font-size: 0.9rem;
  }
  .count {
    margin-left: auto;
    font-size: 0.72rem;
    color: #8a919c;
  }
  .grp {
    margin: 0.35rem 0 0.5rem;
    padding-left: 0.3rem;
    border-left: 2px solid #2b323d;
  }
  .grp-head {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    font-size: 0.82rem;
    color: #cfd4db;
  }
  .jump {
    border: 1px solid #4b5563;
    background: #2b323d;
    color: #cfd4db;
    font-family: inherit;
    font-size: 0.68rem;
    padding: 0 0.3rem;
    cursor: pointer;
  }
  .jump:hover {
    border-color: #4a6da7;
    background: #4a6da7;
    color: #fff;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 0.25rem;
    margin: 0.25rem 0;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 0.25rem;
    border: 1px solid #2f3946;
    background: #1b2027;
    color: #cfd4db;
    font-size: 0.74rem;
    padding: 0.08rem 0.3rem;
  }
  .cdate {
    color: #8a919c;
  }
  .x {
    border: none;
    background: transparent;
    color: #8a919c;
    font-size: 0.9rem;
    line-height: 1;
    cursor: pointer;
    padding: 0;
  }
  .x:hover {
    color: #fca5a5;
  }
  .picker {
    background: #21262e;
    border: 1px solid #1f242c;
    color: #cfd4db;
    font-family: inherit;
    font-size: 0.78rem;
    padding: 0.15rem 0.3rem;
    margin-right: 0.3rem;
  }
  .mini {
    border: 1px solid #4b5563;
    background: #2b323d;
    color: #cfd4db;
    font-family: inherit;
    font-size: 0.72rem;
    padding: 0.1rem 0.4rem;
    cursor: pointer;
  }
  .mini:hover {
    border-color: #4a6da7;
    background: #4a6da7;
    color: #fff;
  }
  .dim {
    color: #9ca3af;
  }
  .small {
    font-size: 0.74rem;
  }
  .err {
    color: #fca5a5;
    font-size: 0.78rem;
  }
</style>
