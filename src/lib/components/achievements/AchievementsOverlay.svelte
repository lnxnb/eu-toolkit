<!--
  AchievementsOverlay — View ▸ Achievements….

  A full-screen OverlaySurface over `common/achievements.txt`: a searchable
  achievement list (icon · name · key · origin badge) → expand editor
  (AchievementEditor: icon, loc name/desc, id/localization, the four trigger
  blocks, preserve-unknown). "＋ New achievement" appends a scaffold to the
  file (copy-on-write into the project) + name/desc loc overrides.

  Honesty note carried in the UI: this edits the IN-GAME achievements window;
  mods can never grant Steam achievements. A TC that blanks the file (Anbennar)
  shows an empty list plus a pointer to its triggered-modifiers convention.
-->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { OverlaySurface } from "$lib/components/script";
  import type { KnownKey } from "$lib/components/script";
  import { ModifierIcon, type DropdownItem } from "$lib/components/ui";
  import type { EditQueue, TypedEdit } from "$lib/edits.svelte";
  import AchievementEditor from "./AchievementEditor.svelte";
  import {
    foldAchievements,
    isValidKey,
    slugify,
    allKeys,
    ACHIEVEMENTS_FILE,
    type AchievementsData,
    type Achievement,
    type Scaffold,
  } from "$lib/achievements";

  let {
    open = $bindable(false),
    installPath,
    modPath = null,
    queue,
    focusKey = null,
    onopenmechanics,
  }: {
    open?: boolean;
    installPath: string;
    modPath?: string | null;
    queue: EditQueue;
    focusKey?: string | null;
    /** Open the Mechanics overlay on a family (triggered_modifiers pointer). */
    onopenmechanics?: (family: string) => void;
  } = $props();

  interface CountryBrief {
    tag: string;
    name: string;
    color: [number, number, number] | null;
  }

  let fetched = $state<AchievementsData | null>(null);
  let triggers = $state<KnownKey[]>([]);
  let countries = $state<DropdownItem[]>([]);
  let loading = $state(false);
  let error = $state<string | null>(null);

  let search = $state("");
  let modOnly = $state(false);
  let expandedKey = $state<string | null>(null);
  let newName = $state("");
  let newError = $state<string | null>(null);

  $effect(() => {
    if (!open) return;
    void load(installPath, modPath);
  });

  // Deep-link focus (search jump): expand once data is in.
  $effect(() => {
    if (focusKey && fetched) expandedKey = focusKey;
  });

  async function load(install: string, mod: string | null) {
    loading = true;
    error = null;
    try {
      const [data, trig, ctys] = await Promise.all([
        invoke<AchievementsData>("get_achievements", { installPath: install, modPath: mod }),
        invoke<KnownKey[]>("get_known_triggers"),
        invoke<CountryBrief[]>("list_countries", { installPath: install, modPath: mod }),
      ]);
      fetched = data;
      triggers = trig;
      countries = ctys.map((c) => ({
        key: c.tag,
        label: c.name,
        swatch: c.color ? `rgb(${c.color[0]}, ${c.color[1]}, ${c.color[2]})` : undefined,
      }));
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  const data = $derived<AchievementsData | null>(
    fetched ? ((queue.version, foldAchievements(fetched, queue.serialize()))) : null,
  );
  const achievements = $derived<Achievement[]>(data?.achievements ?? []);
  const keys = $derived(data ? allKeys(data) : new Set<string>());

  function nameOf(a: Achievement): string {
    return queue.pendingLocOverride(a.nameKey) ?? (a.nameLoc ?? a.name);
  }

  const shown = $derived(
    achievements.filter((a) => {
      if (modOnly && a.origin !== "mod") return false;
      const q = search.trim().toLowerCase();
      if (!q) return true;
      return (
        a.key.toLowerCase().includes(q) ||
        nameOf(a).toLowerCase().includes(q) ||
        (a.localization ?? "").toLowerCase().includes(q)
      );
    }),
  );

  function toggle(k: string) {
    expandedKey = expandedKey === k ? null : k;
  }

  // --- Delete ---
  function removeObject(a: Achievement) {
    if (!confirm(`Delete achievement "${a.key}"?`)) return;
    queue.push({
      label: `Delete achievement ${a.key}`,
      edits: [{ kind: "removeStatement", file: a.file, blockPath: [], key: a.key }],
    });
    if (expandedKey === a.key) expandedKey = null;
  }

  // --- ＋ New … ---
  async function createObject() {
    newError = null;
    const key = slugify(newName.trim());
    if (!isValidKey(key)) {
      newError = "Use lowercase letters, digits and underscores (start with a letter).";
      return;
    }
    if (keys.has(key)) {
      newError = `An achievement named "${key}" already exists.`;
      return;
    }
    let scaffold: Scaffold;
    try {
      scaffold = await invoke<Scaffold>("scaffold_achievement_cmd", { installPath, modPath, key });
    } catch (e) {
      newError = String(e);
      return;
    }
    const edits: TypedEdit[] = [
      { kind: "appendText", file: ACHIEVEMENTS_FILE, text: "\n" + scaffold.text + "\n" },
    ];
    for (const le of scaffold.locEntries) {
      edits.push({ kind: "locOverride", key: le.key, value: le.value });
    }
    queue.push({ label: `Create achievement ${key}`, edits });
    newName = "";
    expandedKey = key;
  }
</script>

<OverlaySurface bind:open title="Achievements">
  {#snippet toolbar()}
    <input class="search" type="text" placeholder="Search name, key…" bind:value={search} />
    <label class="modonly">
      <input type="checkbox" bind:checked={modOnly} />
      Mod only
    </label>
    <span class="counter">{shown.length}</span>
  {/snippet}

  <div class="body">
    <div class="newrow">
      <input class="newkey" type="text" placeholder="New achievement name…" bind:value={newName}
        onkeydown={(e) => e.key === "Enter" && createObject()} />
      <button class="newbtn" onclick={createObject}>＋ New achievement</button>
      {#if newError}<span class="newerr">{newError}</span>{/if}
    </div>
    <p class="steamnote">
      Edits here change the <strong>in-game</strong> achievements window only — mods cannot grant
      Steam achievements (the Steam award is tied to vanilla's compiled id mapping).
    </p>

    {#if loading}
      <p class="msg">Loading achievements…</p>
    {:else if error}
      <p class="msg err">{error}</p>
    {:else if achievements.length === 0}
      <div class="msg">
        <p>
          No achievements — this mod overrides <code>common/achievements.txt</code> with an empty
          file (total conversions blank it because vanilla's achievements can't be earned).
        </p>
        {#if modPath && onopenmechanics}
          <p>
            Anbennar-style custom achievements are paired <code>triggered_modifiers</code>
            (<code>ach_*_g</code> in-progress / <code>ach_*</code> completed) —
            <button class="linkbtn" onclick={() => onopenmechanics("triggered_modifiers")}>
              open Mechanics ▸ Triggered modifiers
            </button>
          </p>
        {/if}
      </div>
    {:else if shown.length === 0}
      <p class="msg">Nothing matches.</p>
    {/if}

    <ul class="list">
      {#each shown as a (a.key)}
        <li class="row" class:expanded={expandedKey === a.key} id={`ach-row-${a.key}`}>
          <button class="rowmain" onclick={() => toggle(a.key)}>
            <span class="caret">{expandedKey === a.key ? "▾" : "▸"}</span>
            <span class="ricon">
              {#if a.hasIcon}
                <ModifierIcon {installPath} {modPath} key={a.key} size="1.5rem" command="get_achievement_icon" />
              {/if}
            </span>
            <span class="title">{nameOf(a)}</span>
            <code class="key">{a.key}</code>
            {#if a.id != null}<span class="aid" title="Steam mapping id">#{a.id}</span>{/if}
            <span class="badge origin {a.origin}">{a.origin}</span>
          </button>
          {#if expandedKey === a.key}
            <div class="rowbody">
              <AchievementEditor
                {installPath}
                {modPath}
                {queue}
                obj={a}
                {triggers}
                {countries}
                onremove={() => removeObject(a)}
              />
            </div>
          {/if}
        </li>
      {/each}
    </ul>
  </div>
</OverlaySurface>

<style>
  .search { background: var(--bg-1); border: 1px solid var(--border); color: var(--text-1); font-family: inherit; font-size: 0.83rem; padding: 0.2rem 0.4rem; width: 16rem; }
  .modonly { display: flex; align-items: center; gap: 0.3rem; font-size: 0.8rem; color: var(--text-1); }
  .counter { font-size: 0.8rem; color: var(--text-2); }
  .body { display: flex; flex-direction: column; gap: 0.5rem; }
  .newrow { display: flex; align-items: center; gap: 0.5rem; flex-wrap: wrap; }
  .newkey { background: var(--bg-1); border: 1px solid var(--border); color: var(--text-1); font-family: inherit; font-size: 0.83rem; padding: 0.25rem 0.4rem; width: 18rem; }
  .newbtn { border: 1px solid var(--border); background: var(--bg-3); color: var(--text-1); font-family: inherit; font-size: 0.82rem; padding: 0.28rem 0.7rem; cursor: pointer; }
  .newbtn:hover { background: var(--accent); color: var(--text-inverse); }
  .newerr { color: var(--err); font-size: 0.78rem; }
  .steamnote { margin: 0; font-size: 0.76rem; color: var(--text-2); }
  .msg { margin: 0.2rem 0; font-size: 0.85rem; color: var(--text-2); }
  .msg.err { color: var(--err); }
  .msg p { margin: 0.2rem 0; }
  .linkbtn { border: none; background: transparent; color: var(--accent); font-family: inherit; font-size: inherit; padding: 0; cursor: pointer; text-decoration: underline; }
  .list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; }
  .row { border: 1px solid var(--border); border-bottom: none; }
  .row:last-child { border-bottom: 1px solid var(--border); }
  .row.expanded { background: var(--bg-2); }
  .rowmain { display: flex; align-items: center; gap: 0.5rem; width: 100%; text-align: left; border: none; background: transparent; color: var(--text-1); font-family: inherit; font-size: 0.86rem; padding: 0.25rem 0.5rem; cursor: pointer; }
  .rowmain:hover { background: var(--bg-3); }
  .caret { color: var(--text-2); width: 0.8rem; flex: none; }
  .ricon { width: 1.5rem; height: 1.5rem; flex: none; display: flex; align-items: center; justify-content: center; }
  .title { font-weight: 600; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 22rem; }
  .key { color: var(--ok); background: var(--bg-0); padding: 0 0.3rem; font-size: 0.76rem; }
  .aid { color: var(--text-3); font-size: 0.72rem; }
  .badge { margin-left: auto; font-size: 0.68rem; text-transform: uppercase; letter-spacing: 0.03em; padding: 0.05rem 0.35rem; border: 1px solid var(--border); }
  .badge.origin.base { background: var(--bg-3); color: var(--text-1); }
  .badge.origin.mod { background: var(--ok); color: var(--text-inverse); }
  .rowbody { padding: 0 0.6rem 0.4rem; }
</style>
