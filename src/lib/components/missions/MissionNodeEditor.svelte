<!--
  MissionNodeEditor — the side panel for one selected mission node (Sprint 17).

  Mirrors the EventEditor pattern: trigger / effect / provinces_to_highlight blocks
  are parsed through parse_script_block_with_edits (folding the PENDING queue) and
  edited with the shared ScriptTreeEditor. Loc (title/desc), icon (SpritePicker over
  `mission_*`), completed_by and the required_missions links are reported up to the
  host, which pushes the byte-surgical edits AND updates the board's working model.

  provinces_to_highlight is a genuine TRIGGER block in vanilla (e.g.
  `{ owned_by = NOV province_id = 310 }`), so it is edited as a trigger tree (which
  carries the raw/tree toggle the spec asks for) rather than a bare id list — adding
  a `province_id = N` condition is the add-by-number path. Map-click-to-add is out
  of scope here (no capital-picker hook is reachable from inside the overlay).
-->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { ScriptTreeEditor, SpritePicker } from "$lib/components/script";
  import type { KnownKey, ScriptBlock } from "$lib/components/script";
  import type { DropdownItem } from "$lib/components/ui";
  import type { EditQueue, TypedEdit } from "$lib/edits.svelte";
  import { MISSION_ICON_PREFIX, type MissionSeries, type MissionEntry } from "./missionsTypes";

  let {
    mission,
    series,
    installPath,
    modPath = null,
    queue,
    triggers,
    effects,
    countries = [],
    onseticon,
    onsetcompletedby,
    onunlink,
    ondelete,
  }: {
    mission: MissionEntry;
    series: MissionSeries;
    installPath: string;
    modPath?: string | null;
    queue: EditQueue;
    triggers: KnownKey[];
    effects: KnownKey[];
    countries?: DropdownItem[];
    onseticon: (key: string, icon: string) => void;
    onsetcompletedby: (key: string, value: string) => void;
    onunlink: (dependent: string, prereq: string) => void;
    ondelete: (key: string) => void;
  } = $props();

  const file = $derived(series.file);

  // --- Parsed sub-blocks (re-parsed on queue change) -----------------------
  // Presence is derived from the freshly-parsed mission block (not the static
  // has* flags), so a just-added trigger/effect/provinces block shows at once.
  let triggerBlock = $state<ScriptBlock | null>(null);
  let effectBlock = $state<ScriptBlock | null>(null);
  let provincesBlock = $state<ScriptBlock | null>(null);

  async function parseBlock(path: string[]): Promise<ScriptBlock | null> {
    try {
      return await invoke<ScriptBlock>("parse_script_block_with_edits", {
        installPath,
        modPath,
        file,
        path,
        edits: queue.serialize(),
      });
    } catch {
      return null;
    }
  }

  let loadToken = 0;
  $effect(() => {
    void installPath;
    void modPath;
    void mission.key;
    queue.version;
    const token = ++loadToken;
    void reload(token);
  });

  async function reload(token: number) {
    const mb = await parseBlock(mission.path);
    if (token !== loadToken) return;
    const has = (k: string) => (mb?.nodes ?? []).some((n) => n.key === k);
    triggerBlock = has("trigger") ? await parseBlock(mission.triggerPath) : null;
    if (token !== loadToken) return;
    effectBlock = has("effect") ? await parseBlock(mission.effectPath) : null;
    if (token !== loadToken) return;
    provincesBlock = has("provinces_to_highlight") ? await parseBlock(mission.provincesPath) : null;
  }

  // --- Loc (title / desc) --------------------------------------------------
  const titleValue = $derived(queue.pendingLocOverride(mission.titleKey) ?? mission.titleLoc ?? "");
  const descValue = $derived(queue.pendingLocOverride(mission.descKey) ?? mission.descLoc ?? "");
  function commitLoc(key: string, value: string, current: string) {
    if (value === current) return;
    queue.push({ label: "Edit mission text", edits: [{ kind: "locOverride", key, value }] });
  }

  // --- completed_by --------------------------------------------------------
  const completedValue = $derived(mission.completedBy ?? "");

  // --- Icon picker ---------------------------------------------------------
  let pickingIcon = $state(false);
  function commitIcon(name: string) {
    onseticon(mission.key, name);
    pickingIcon = false;
  }

  // --- Script tree add-block helpers (absent → insert an empty block) -------
  function pushEdits(edits: TypedEdit[], label: string) {
    if (edits.length) queue.push({ label, edits });
  }
  function onTreeEdit(edits: TypedEdit[], label: string) {
    pushEdits(edits, label);
  }
  function addBlock(name: string, body: string, label: string) {
    pushEdits(
      [{ kind: "insertStatement", file, blockPath: mission.path, statement: `${name} = {\n${body}}` }],
      label,
    );
  }
</script>

<div class="ned">
  <div class="idrow">
    <code class="id">{mission.key}</code>
    <span class="pos">row {mission.effectivePosition}</span>
    {#if mission.pendingBadge}<span class="badge pending">unsaved</span>{/if}
  </div>

  <label class="fld">
    <span>Title</span>
    <input type="text" value={titleValue} placeholder={mission.titleKey}
      onchange={(e) => commitLoc(mission.titleKey, e.currentTarget.value, titleValue)} />
  </label>
  <label class="fld">
    <span>Description</span>
    <textarea rows="2" value={descValue} placeholder={mission.descKey}
      onchange={(e) => commitLoc(mission.descKey, e.currentTarget.value, descValue)}></textarea>
  </label>

  <!-- Icon -->
  <div class="fld">
    <span>Icon</span>
    <div class="picrow">
      <code class="mono">{mission.icon ?? "(none)"}</code>
      <button class="mini" onclick={() => (pickingIcon = !pickingIcon)}>{pickingIcon ? "Close" : "Change…"}</button>
    </div>
    {#if pickingIcon}
      <div class="picker">
        <SpritePicker {installPath} {modPath} prefix={MISSION_ICON_PREFIX} value={mission.icon} onselect={commitIcon} />
      </div>
    {/if}
  </div>

  <!-- completed_by -->
  <label class="fld">
    <span>Completed by <span class="adv">(auto-complete date, optional)</span></span>
    <input type="text" value={completedValue} placeholder="e.g. 1478.1.15"
      onchange={(e) => onsetcompletedby(mission.key, e.currentTarget.value.trim())} />
  </label>

  <!-- required_missions -->
  <section class="block">
    <h4>Requires</h4>
    {#if mission.requiredMissions.length === 0}
      <p class="note">No prerequisites (root mission). Use Link mode on the board to add one.</p>
    {:else}
      <div class="reqs">
        {#each mission.requiredMissions as req (req)}
          <span class="req">
            <code>{req}</code>
            <button class="unlink" title="Remove this requirement" onclick={() => onunlink(mission.key, req)}>×</button>
          </span>
        {/each}
      </div>
    {/if}
  </section>

  <!-- provinces_to_highlight (a trigger block) -->
  <section class="block">
    <h4>Provinces to highlight <span class="adv">highlight condition</span></h4>
    {#if provincesBlock}
      <ScriptTreeEditor file={file} rootPath={mission.provincesPath} block={provincesBlock}
        registry="triggers" known={triggers} {countries} onedit={onTreeEdit} />
    {:else}
      <button class="add-block" onclick={() => addBlock("provinces_to_highlight", "\t\t\tprovince_id = 1\n\t\t", "Add provinces_to_highlight")}>
        ＋ Add provinces_to_highlight
      </button>
    {/if}
  </section>

  <!-- trigger -->
  <section class="block">
    <h4>Completion trigger</h4>
    {#if triggerBlock}
      <ScriptTreeEditor file={file} rootPath={mission.triggerPath} block={triggerBlock}
        registry="triggers" known={triggers} {countries} onedit={onTreeEdit} />
    {:else}
      <button class="add-block" onclick={() => addBlock("trigger", "\t\t", "Add trigger block")}>＋ Add trigger block</button>
    {/if}
  </section>

  <!-- effect -->
  <section class="block">
    <h4>Effect on completion</h4>
    {#if effectBlock}
      <ScriptTreeEditor file={file} rootPath={mission.effectPath} block={effectBlock}
        registry="effects" known={effects} {countries} onedit={onTreeEdit} />
    {:else}
      <button class="add-block" onclick={() => addBlock("effect", "\t\t", "Add effect block")}>＋ Add effect block</button>
    {/if}
  </section>

  <section class="block danger">
    <button class="del" onclick={() => ondelete(mission.key)}>Delete mission</button>
  </section>
</div>

<style>
  .ned { display: flex; flex-direction: column; gap: 0.5rem; }
  .idrow { display: flex; align-items: center; gap: 0.5rem; }
  .id { color: var(--ok); background: var(--bg-0); padding: 0.05rem 0.35rem; font-size: 0.8rem; }
  .pos { font-size: 0.72rem; color: var(--text-2); }

  .fld { display: flex; flex-direction: column; gap: 0.15rem; font-size: 0.76rem; color: var(--text-2); }
  .fld input, .fld textarea {
    background: var(--bg-1); border: 1px solid var(--border); color: var(--text-1);
    font-family: inherit; font-size: 0.84rem; padding: 0.25rem 0.35rem; resize: vertical;
  }
  .adv { text-transform: none; color: var(--text-3); font-size: 0.7rem; }

  .picrow { display: flex; align-items: center; gap: 0.5rem; }
  .mono { color: var(--text-1); background: var(--bg-0); padding: 0.05rem 0.35rem; font-size: 0.74rem; }
  .mini {
    border: 1px solid var(--border); background: var(--bg-3); color: var(--text-1);
    font-family: inherit; font-size: 0.74rem; padding: 0.1rem 0.5rem; cursor: pointer;
  }
  .mini:hover { background: var(--accent); color: var(--text-inverse); }
  .picker { height: 20rem; margin-top: 0.3rem; }

  .block { border-top: 1px solid var(--border); padding-top: 0.4rem; }
  .block h4 {
    margin: 0 0 0.35rem; font-size: 0.76rem; text-transform: uppercase;
    letter-spacing: 0.04em; color: var(--text-2);
  }
  .note { margin: 0.1rem 0; font-size: 0.78rem; color: var(--text-2); }

  .reqs { display: flex; flex-wrap: wrap; gap: 0.3rem; }
  .req {
    display: inline-flex; align-items: center; gap: 0.3rem;
    border: 1px solid var(--border); background: var(--bg-1); padding: 0.1rem 0.3rem;
  }
  .req code { font-size: 0.74rem; color: var(--text-1); }
  .unlink {
    border: none; background: transparent; color: var(--err);
    font-size: 0.95rem; line-height: 1; cursor: pointer; padding: 0;
  }
  .unlink:hover { color: var(--err); }

  .add-block {
    align-self: flex-start; border: 1px solid var(--border); background: var(--bg-3); color: var(--text-1);
    font-family: inherit; font-size: 0.78rem; padding: 0.25rem 0.6rem; cursor: pointer;
  }
  .add-block:hover { background: var(--accent); color: var(--text-inverse); }

  .danger { display: flex; }
  .del {
    border: 1px solid var(--danger-bg); background: var(--bg-1); color: var(--err);
    font-family: inherit; font-size: 0.78rem; padding: 0.25rem 0.7rem; cursor: pointer;
  }
  .del:hover { background: var(--danger-bg); color: var(--text-inverse); }
</style>
