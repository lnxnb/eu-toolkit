<!--
  MissionsOverlay — View ▸ Missions… (Sprint 17, combined-board rework).

  Two landing tabs inside the shared OverlaySurface:
    • By country — pick a country; you land STRAIGHT on the in-game combined board
      (MissionBoard) composed of every series that country receives (the definite
      "yes" bucket from evaluate_series_potential), grouped into the five slot
      columns with same-slot series stacked. Potentials are evaluated once (batch,
      snapshot-built-once) and cached, so re-picking is instant.
    • All series — every series grouped by file; opening one shows the SAME board
      component with a list of one series.

  Board view = a series-settings header (for the selected series: slot / generic /
  ai / has_country_shield / potential) + the combined MissionBoard + a node editor
  (MissionNodeEditor) + a collapsed "approximate" section (country tab) whose
  series can be added to the board on demand + a "create tree" call-to-action for
  countries with no series.

  The overlay owns the board's optimistic working model: every structural edit
  targets the OWNING series' file exactly (paths carry file + series), both pushing
  the byte-surgical composite to the shared queue AND mutating the working series so
  the board reflects it immediately. Cross-series links/cycles are resolved over the
  union of all displayed series (missionLayout.ts).
-->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { OverlaySurface } from "$lib/components/script";
  import { ScriptTreeEditor } from "$lib/components/script";
  import type { KnownKey, ScriptBlock } from "$lib/components/script";
  import type { DropdownItem } from "$lib/components/ui";
  import type { EditQueue, TypedEdit } from "$lib/edits.svelte";
  import { getFlagUrl } from "$lib/flagCache";
  import MissionBoard from "./MissionBoard.svelte";
  import MissionNodeEditor from "./MissionNodeEditor.svelte";
  import { combinedEdges, combinedCreatesCycle } from "./missionLayout";
  import { shouldApplySelection, type Selection } from "./selectionGuard";
  import {
    SCAFFOLD_FILE,
    seriesId,
    type MissionSeries,
    type MissionEntry,
    type SeriesPotential,
  } from "./missionsTypes";

  let {
    open = $bindable(false),
    installPath,
    modPath = null,
    selectedDate = null,
    queue,
  }: {
    open?: boolean;
    installPath: string;
    modPath?: string | null;
    selectedDate?: string | null;
    queue: EditQueue;
  } = $props();

  interface CountryBrief {
    tag: string;
    name: string;
    color: [number, number, number] | null;
  }

  let fetched = $state<MissionSeries[]>([]);
  let pendingCreated = $state<MissionSeries[]>([]);
  // Per-tag potential cache: the per-country board needs only ONE tag's verdicts,
  // so each country is evaluated on demand (incl. formables that don't exist at
  // the date) and cached by tag. Keyed by tag → its series potentials.
  let potByTag = $state<Record<string, SeriesPotential[]>>({});
  let potLoadingTag = $state<string | null>(null);
  let countries = $state<CountryBrief[]>([]);
  let triggers = $state<KnownKey[]>([]);
  let effects = $state<KnownKey[]>([]);
  let loading = $state(false);
  let error = $state<string | null>(null);

  let tab = $state<"country" | "all">("country");
  let search = $state("");
  let selectedTag = $state<string | null>(null);

  // Board view state — a LIST of working series (the combined per-country board,
  // or a single series from the All-series tab).
  let boardSeries = $state<MissionSeries[] | null>(null);
  let boardKind = $state<"country" | "single">("country");
  let boardTag = $state<string | null>(null);
  // The board is open in a LOADING state (a country was clicked and its potentials
  // are still being evaluated). Distinct from `boardSeries = []` (genuinely empty).
  let boardLoading = $state(false);
  let selectedSeriesIndex = $state(0);
  let selectedNode = $state<{ seriesIndex: number; key: string } | null>(null);
  let boardMessage = $state<string | null>(null);

  // Monotonic guard for country selection (race-safe board population).
  let selectSeq = 0;
  let currentSelection = $state<Selection>({ seq: 0, tag: null });

  // The board view is active whenever a board is loading or has data.
  const inBoard = $derived(boardLoading || boardSeries != null);

  const countryItems = $derived<DropdownItem[]>(
    countries.map((c) => ({
      key: c.tag,
      label: c.name,
      swatch: c.color ? `rgb(${c.color[0]}, ${c.color[1]}, ${c.color[2]})` : undefined,
    })),
  );
  const countryByTag = $derived(new Map(countries.map((c) => [c.tag, c])));

  $effect(() => {
    if (!open) return;
    void load(installPath, modPath);
  });

  async function load(install: string, mod: string | null) {
    loading = true;
    error = null;
    try {
      const [ser, ctys, trig, eff] = await Promise.all([
        invoke<MissionSeries[]>("get_mission_series", { installPath: install, modPath: mod }),
        invoke<CountryBrief[]>("list_countries", { installPath: install, modPath: mod }),
        invoke<KnownKey[]>("get_known_triggers"),
        invoke<KnownKey[]>("get_known_effects"),
      ]);
      fetched = ser;
      countries = ctys;
      triggers = trig;
      effects = eff;
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  const allSeries = $derived<MissionSeries[]>([...pendingCreated, ...fetched]);

  // --- Potential evaluation (per-tag, cached) ------------------------------
  // Evaluates ONE tag's series potentials via the backend `tag` filter. Includes
  // formables (tags that don't exist at the date). Cached by tag; a second click
  // on the same country is instant. Returns the loaded potentials (or [] on error).
  async function loadPotentialsFor(tag: string): Promise<SeriesPotential[]> {
    const cached = potByTag[tag];
    if (cached) return cached;
    potLoadingTag = tag;
    try {
      const res = await invoke<SeriesPotential[]>("evaluate_series_potential", {
        installPath,
        modPath,
        date: selectedDate,
        tag,
      });
      potByTag = { ...potByTag, [tag]: res };
      return res;
    } catch (e) {
      boardMessage = `Could not evaluate potentials: ${e}`;
      return [];
    } finally {
      if (potLoadingTag === tag) potLoadingTag = null;
    }
  }

  // Selected date changes invalidate every tag's cached verdicts.
  let lastPotDate: string | null | undefined = undefined;
  $effect(() => {
    if (lastPotDate !== undefined && selectedDate !== lastPotDate) {
      potByTag = {};
    }
    lastPotDate = selectedDate;
  });

  // Series a tag receives (definite "yes") / possibly (approximate "unknown"),
  // from that tag's cached potentials.
  function receivesFor(tag: string): { yes: MissionSeries[]; maybe: MissionSeries[] } {
    const pots = potByTag[tag] ?? [];
    const potByKey = new Map<string, SeriesPotential>();
    for (const p of pots) potByKey.set(`${p.file}::${p.key}`, p);
    const yes: MissionSeries[] = [];
    const maybe: MissionSeries[] = [];
    for (const s of allSeries) {
      const p = potByKey.get(seriesId(s));
      if (!p) continue; // pending/unevaluated handled separately
      if (p.yes.includes(tag)) yes.push(s);
      else if (p.unknown.includes(tag)) maybe.push(s);
    }
    return { yes, maybe };
  }

  // --- All-series tab: grouped by file -------------------------------------
  const grouped = $derived.by(() => {
    const q = search.trim().toLowerCase();
    const filtered = allSeries.filter(
      (s) =>
        !q ||
        s.key.toLowerCase().includes(q) ||
        s.file.toLowerCase().includes(q) ||
        s.missions.some((m) => m.title.toLowerCase().includes(q)),
    );
    const map = new Map<string, MissionSeries[]>();
    for (const s of filtered) {
      const arr = map.get(s.file) ?? [];
      arr.push(s);
      map.set(s.file, arr);
    }
    return [...map.entries()].map(([file, list]) => ({ file, list }));
  });

  // --- Lazy country flags (mirror SpritePicker's IntersectionObserver) ------
  let flagUrls = $state<Record<string, string | null>>({});
  let flagObserver: IntersectionObserver | null = null;
  const flagEls = new WeakMap<Element, string>();
  function ensureFlagObserver(root: Element) {
    if (flagObserver) return;
    flagObserver = new IntersectionObserver(
      (entries) => {
        for (const e of entries) {
          if (!e.isIntersecting) continue;
          const tag = flagEls.get(e.target);
          if (tag) void loadFlag(tag);
        }
      },
      { root, rootMargin: "150px" },
    );
  }
  function lazyFlag(el: HTMLElement, tag: string) {
    const root = el.closest(".country-list") ?? el.parentElement ?? el;
    ensureFlagObserver(root);
    flagEls.set(el, tag);
    flagObserver?.observe(el);
    return {
      update(next: string) {
        flagEls.set(el, next);
      },
      destroy() {
        flagObserver?.unobserve(el);
      },
    };
  }
  async function loadFlag(tag: string) {
    if (tag in flagUrls) return;
    flagUrls = { ...flagUrls, [tag]: null };
    const url = await getFlagUrl(installPath, modPath, tag);
    flagUrls = { ...flagUrls, [tag]: url };
  }

  // --- Open / close board --------------------------------------------------
  function cloneSeries(s: MissionSeries): MissionSeries {
    return structuredClone($state.snapshot(s)) as MissionSeries;
  }

  async function selectCountry(tag: string) {
    selectedTag = tag;
    // Open the board immediately in a loading state, keyed to this tag, and record
    // the selection so a stale response for a previously-clicked tag can't populate
    // the wrong board (mirrors MapView's renderSeq guard).
    const req: Selection = { seq: ++selectSeq, tag };
    currentSelection = req;
    boardKind = "country";
    boardTag = tag;
    boardSeries = null;
    boardLoading = true;
    selectedSeriesIndex = 0;
    selectedNode = null;
    boardMessage = null;

    await loadPotentialsFor(tag);
    // Drop if a newer selection superseded this one.
    if (!shouldApplySelection(req, currentSelection)) return;
    boardLoading = false;
    openCountryBoard(tag);
  }
  function openCountryBoard(tag: string) {
    const r = receivesFor(tag);
    const pend = pendingCreated.filter(
      (s) => s.pendingTag === tag && !r.yes.some((y) => seriesId(y) === seriesId(s)),
    );
    boardSeries = [...r.yes, ...pend].map(cloneSeries);
    boardKind = "country";
    boardTag = tag;
    boardLoading = false;
    selectedSeriesIndex = 0;
    selectedNode = null;
    boardMessage = null;
  }
  function openSingleSeries(s: MissionSeries) {
    boardSeries = [cloneSeries(s)];
    boardKind = "single";
    boardTag = null;
    boardLoading = false;
    selectedSeriesIndex = 0;
    selectedNode = null;
    boardMessage = null;
  }
  function backToLanding() {
    // Abandon any in-flight selection so its response won't reopen a board.
    currentSelection = { seq: ++selectSeq, tag: null };
    boardSeries = null;
    boardLoading = false;
    selectedNode = null;
    boardMessage = null;
  }

  const selSeries = $derived<MissionSeries | null>(
    boardSeries && selectedSeriesIndex >= 0 && selectedSeriesIndex < boardSeries.length
      ? boardSeries[selectedSeriesIndex]
      : null,
  );
  const selMission = $derived<MissionEntry | null>(
    boardSeries && selectedNode
      ? (boardSeries[selectedNode.seriesIndex]?.missions.find((m) => m.key === selectedNode!.key) ?? null)
      : null,
  );
  const selNodeSeries = $derived<MissionSeries | null>(
    boardSeries && selectedNode ? (boardSeries[selectedNode.seriesIndex] ?? null) : null,
  );

  // Approximate (unknown-bucket) series not already on the board.
  const approxSeries = $derived.by<MissionSeries[]>(() => {
    if (boardKind !== "country" || !boardTag || !boardSeries) return [];
    const shown = new Set(boardSeries.map((s) => seriesId(s)));
    return receivesFor(boardTag).maybe.filter((s) => !shown.has(seriesId(s)));
  });
  function addApproxSeries(s: MissionSeries) {
    if (!boardSeries) return;
    const clone = cloneSeries(s);
    clone.approx = true;
    boardSeries = [...boardSeries, clone];
    selectedSeriesIndex = boardSeries.length - 1;
    selectedNode = null;
  }

  // --- Series-block parse (for series-scalar presence + potential editor) ---
  let seriesBlock = $state<ScriptBlock | null>(null);
  let potentialBlock = $state<ScriptBlock | null>(null);
  let sbToken = 0;
  async function parseBlock(file: string, path: string[]): Promise<ScriptBlock | null> {
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
  $effect(() => {
    const w = selSeries;
    queue.version;
    if (!w) {
      seriesBlock = potentialBlock = null;
      return;
    }
    const token = ++sbToken;
    void (async () => {
      const sb = await parseBlock(w.file, w.path);
      if (token !== sbToken) return;
      seriesBlock = sb;
      potentialBlock = w.hasPotential ? await parseBlock(w.file, w.potentialPath) : null;
    })();
  });
  const seriesPresent = $derived(new Set((seriesBlock?.nodes ?? []).map((n) => n.key)));

  // --- Edit helpers (push queue + mutate working) --------------------------
  function bump() {
    boardSeries = boardSeries ? [...boardSeries] : null;
  }
  function push(edits: TypedEdit[], label: string) {
    if (edits.length) queue.push({ label, edits });
  }
  function seriesAt(i: number): MissionSeries | undefined {
    return boardSeries?.[i];
  }
  function findM(i: number, key: string): MissionEntry | undefined {
    return seriesAt(i)?.missions.find((m) => m.key === key);
  }

  function moveMission(i: number, key: string, position: number) {
    const w = seriesAt(i);
    const m = findM(i, key);
    if (!w || !m || position < 1) return;
    if (m.position === position) return;
    const edit: TypedEdit =
      m.position != null
        ? { kind: "setScalar", file: w.file, path: [...m.path, "position"], value: String(position), quoted: false }
        : { kind: "insertStatement", file: w.file, blockPath: m.path, statement: `position = ${position}` };
    push([edit], `Move ${key} to row ${position}`);
    m.position = position;
    m.effectivePosition = position;
    bump();
  }

  function linkMissions(depI: number, dependent: string, prereq: string) {
    const w = seriesAt(depI);
    const dep = findM(depI, dependent);
    if (!w || !dep) return;
    const edges = combinedEdges(boardSeries ?? []);
    if (combinedCreatesCycle(edges, dependent, prereq)) {
      boardMessage = `Rejected: linking "${prereq}" → "${dependent}" would create a requirement cycle.`;
      return;
    }
    if (dep.requiredMissions.includes(prereq)) {
      boardMessage = `"${dependent}" already requires "${prereq}".`;
      return;
    }
    const edits: TypedEdit[] = [];
    if (!dep.hasRequiredBlock) {
      edits.push({ kind: "insertStatement", file: w.file, blockPath: dep.path, statement: "required_missions = { }" });
    }
    edits.push({ kind: "addId", file: w.file, listPath: dep.requiredPath, id: prereq });
    push(edits, `Require ${prereq} for ${dependent}`);
    dep.requiredMissions = [...dep.requiredMissions, prereq];
    dep.hasRequiredBlock = true;
    boardMessage = null;
    bump();
  }

  function unlinkMission(depI: number, dependent: string, prereq: string) {
    const w = seriesAt(depI);
    const dep = findM(depI, dependent);
    if (!w || !dep) return;
    push([{ kind: "removeId", file: w.file, listPath: dep.requiredPath, id: prereq }], `Unlink ${prereq} from ${dependent}`);
    dep.requiredMissions = dep.requiredMissions.filter((r) => r !== prereq);
    bump();
  }

  function setIcon(i: number, key: string, icon: string) {
    const w = seriesAt(i);
    const m = findM(i, key);
    if (!w || !m) return;
    const edit: TypedEdit =
      m.icon != null
        ? { kind: "setScalar", file: w.file, path: [...m.path, "icon"], value: icon, quoted: false }
        : { kind: "insertStatement", file: w.file, blockPath: m.path, statement: `icon = ${icon}` };
    push([edit], `Set icon for ${key}`);
    m.icon = icon;
    bump();
  }

  function setCompletedBy(i: number, key: string, value: string) {
    const w = seriesAt(i);
    const m = findM(i, key);
    if (!w || !m) return;
    if (value === "") {
      if (m.completedBy != null) {
        push([{ kind: "removeStatement", file: w.file, blockPath: m.path, key: "completed_by", value: null }], `Clear completed_by for ${key}`);
        m.completedBy = null;
        bump();
      }
      return;
    }
    const edit: TypedEdit =
      m.completedBy != null
        ? { kind: "setScalar", file: w.file, path: [...m.path, "completed_by"], value, quoted: false }
        : { kind: "insertStatement", file: w.file, blockPath: m.path, statement: `completed_by = ${value}` };
    push([edit], `Set completed_by for ${key}`);
    m.completedBy = value;
    bump();
  }

  // --- Add mission (click an empty cell) -----------------------------------
  let addingAt = $state<number | null>(null);
  let addingSeriesIndex = $state(0);
  let addKey = $state("");
  let addError = $state<string | null>(null);
  const KEY_RE = /^[a-z][a-z0-9_]*$/;

  function beginAdd(i: number, position: number) {
    addingSeriesIndex = i;
    addingAt = position;
    addKey = "";
    addError = null;
    selectedNode = null;
  }
  function confirmAdd() {
    const w = seriesAt(addingSeriesIndex);
    if (!w || addingAt == null) return;
    const key = addKey.trim();
    if (!KEY_RE.test(key)) {
      addError = "Key: lowercase letters, digits and underscores (start with a letter).";
      return;
    }
    if (boardSeries?.some((s) => s.missions.some((m) => m.key === key))) {
      addError = "A mission with this key already exists on the board.";
      return;
    }
    const position = addingAt;
    const titleKey = `${key}_title`;
    const descKey = `${key}_desc`;
    const body =
      `${key} = {\n` +
      `\t\ticon = mission_build_up_to_force_limit\n` +
      `\t\tposition = ${position}\n` +
      `\t\trequired_missions = { }\n` +
      `\t\ttrigger = {\n\t\t}\n` +
      `\t\teffect = {\n\t\t}\n` +
      `\t}`;
    push(
      [
        { kind: "insertStatement", file: w.file, blockPath: w.path, statement: body },
        { kind: "locOverride", key: titleKey, value: "New Mission" },
        { kind: "locOverride", key: descKey, value: "" },
      ],
      `Add mission ${key}`,
    );
    const path = [...w.path, key];
    const entry: MissionEntry = {
      key,
      icon: "mission_build_up_to_force_limit",
      position,
      ordinal: w.missions.length + 1,
      effectivePosition: position,
      requiredMissions: [],
      completedBy: null,
      title: "New Mission",
      titleKey,
      descKey,
      titleLoc: "New Mission",
      descLoc: "",
      path,
      triggerPath: [...path, "trigger"],
      effectPath: [...path, "effect"],
      provincesPath: [...path, "provinces_to_highlight"],
      requiredPath: [...path, "required_missions"],
      hasTrigger: true,
      hasEffect: true,
      hasProvinces: false,
      hasRequiredBlock: true,
      pendingBadge: true,
    };
    w.missions = [...w.missions, entry];
    bump();
    selectedNode = { seriesIndex: addingSeriesIndex, key };
    addingAt = null;
  }

  // --- Delete mission (cleanup dependents across ALL displayed series) ------
  function deleteMission(i: number, key: string) {
    const w = seriesAt(i);
    const m = findM(i, key);
    if (!w || !m || !boardSeries) return;
    if (!confirm(`Delete mission "${key}"? Dependent missions will have this requirement removed.`)) return;
    const missionSeg = m.path[m.path.length - 1];
    const edits: TypedEdit[] = [
      { kind: "removeStatement", file: w.file, blockPath: w.path, key: missionSeg, value: null },
    ];
    // Clean dependents' required_missions across every displayed series/file.
    for (const bs of boardSeries) {
      for (const other of bs.missions) {
        if (other.key !== key && other.requiredMissions.includes(key)) {
          edits.push({ kind: "removeId", file: bs.file, listPath: other.requiredPath, id: key });
        }
      }
    }
    push(edits, `Delete mission ${key}`);
    boardSeries = boardSeries.map((bs, bi) => {
      const missions = (bi === i ? bs.missions.filter((x) => x.key !== key) : bs.missions).map((x) => ({
        ...x,
        requiredMissions: x.requiredMissions.filter((r) => r !== key),
      }));
      return { ...bs, missions };
    });
    if (selectedNode?.key === key) selectedNode = null;
  }

  // --- Series-level flag / slot edits (operate on the SELECTED series) ------
  function toggleSeriesFlag(flag: "generic" | "ai" | "has_country_shield", current: boolean) {
    const w = selSeries;
    if (!w) return;
    const target = !current;
    const edit: TypedEdit = seriesPresent.has(flag)
      ? { kind: "setScalar", file: w.file, path: [...w.path, flag], value: target ? "yes" : "no", quoted: false }
      : { kind: "insertStatement", file: w.file, blockPath: w.path, statement: `${flag} = ${target ? "yes" : "no"}` };
    push([edit], `${target ? "Set" : "Unset"} ${flag}`);
    if (flag === "generic") w.generic = target;
    else if (flag === "ai") w.ai = target;
    else w.hasCountryShield = target;
    bump();
  }
  function setSlot(value: number) {
    const w = selSeries;
    if (!w || value < 1 || value > 5) return;
    const edit: TypedEdit = seriesPresent.has("slot")
      ? { kind: "setScalar", file: w.file, path: [...w.path, "slot"], value: String(value), quoted: false }
      : { kind: "insertStatement", file: w.file, blockPath: w.path, statement: `slot = ${value}` };
    push([edit], `Set slot ${value}`);
    w.slot = value;
    bump();
  }
  function addPotential() {
    const w = selSeries;
    if (!w) return;
    push([{ kind: "insertStatement", file: w.file, blockPath: w.path, statement: "potential = {\n\t}" }], "Add potential block");
    w.hasPotential = true;
    bump();
  }
  function onTreeEdit(edits: TypedEdit[], label: string) {
    push(edits, label);
  }

  // --- + New series --------------------------------------------------------
  let creating = $state(false);
  let newKey = $state("");
  let newSlot = $state(1);
  let newError = $state<string | null>(null);

  const scaffoldExists = $derived(
    fetched.some((s) => s.file === SCAFFOLD_FILE) ||
      pendingCreated.some((s) => s.file === SCAFFOLD_FILE) ||
      queue.findLast((e) => e.kind === "createFile" && e.file === SCAFFOLD_FILE) != null,
  );

  // The tag a new series should be gated to: the open board's country, else the
  // selected country in the list.
  const createTag = $derived(boardTag ?? selectedTag);

  function beginCreate() {
    creating = true;
    newKey = createTag ? `${createTag.toLowerCase()}_missions` : "my_missions";
    newSlot = 1;
    newError = null;
  }
  function confirmCreate() {
    const key = newKey.trim();
    if (!KEY_RE.test(key)) {
      newError = "Key: lowercase letters, digits and underscores (start with a letter).";
      return;
    }
    if (allSeries.some((s) => s.key === key) || boardSeries?.some((s) => s.key === key)) {
      newError = "A series with this key already exists.";
      return;
    }
    const tag = createTag;
    const missionKey = `${key}_1`;
    const potentialBody = tag ? `tag = ${tag}` : "always = yes";
    const seriesBody =
      `${key} = {\n` +
      `\tslot = ${newSlot}\n` +
      `\tgeneric = no\n` +
      `\tai = yes\n` +
      `\thas_country_shield = yes\n` +
      `\tpotential = {\n\t\t${potentialBody}\n\t}\n` +
      `\t${missionKey} = {\n` +
      `\t\ticon = mission_build_up_to_force_limit\n` +
      `\t\tposition = 1\n` +
      `\t\trequired_missions = { }\n` +
      `\t\ttrigger = {\n\t\t}\n` +
      `\t\teffect = {\n\t\t}\n` +
      `\t}\n` +
      `}`;

    const edits: TypedEdit[] = [];
    if (!scaffoldExists) {
      edits.push({ kind: "createFile", file: SCAFFOLD_FILE, text: `${seriesBody}\n` });
    } else {
      edits.push({ kind: "insertStatement", file: SCAFFOLD_FILE, blockPath: [], statement: seriesBody });
    }
    edits.push({ kind: "locOverride", key: `${missionKey}_title`, value: "New Mission" });
    edits.push({ kind: "locOverride", key: `${missionKey}_desc`, value: "" });
    queue.push({ label: `Create mission series ${key}`, edits });

    const path = [key];
    const mpath = [key, missionKey];
    const entry: MissionEntry = {
      key: missionKey,
      icon: "mission_build_up_to_force_limit",
      position: 1,
      ordinal: 1,
      effectivePosition: 1,
      requiredMissions: [],
      completedBy: null,
      title: "New Mission",
      titleKey: `${missionKey}_title`,
      descKey: `${missionKey}_desc`,
      titleLoc: "New Mission",
      descLoc: "",
      path: mpath,
      triggerPath: [...mpath, "trigger"],
      effectPath: [...mpath, "effect"],
      provincesPath: [...mpath, "provinces_to_highlight"],
      requiredPath: [...mpath, "required_missions"],
      hasTrigger: true,
      hasEffect: true,
      hasProvinces: false,
      hasRequiredBlock: true,
      pendingBadge: true,
    };
    const series: MissionSeries = {
      key,
      file: SCAFFOLD_FILE,
      origin: "mod",
      slot: newSlot,
      generic: false,
      ai: true,
      hasCountryShield: true,
      hasPotential: true,
      path,
      potentialPath: [key, "potential"],
      missions: [entry],
      pending: true,
      pendingTag: tag ?? undefined,
    };
    pendingCreated = [series, ...pendingCreated];
    creating = false;

    if (boardSeries) {
      // In a board view: drop the new series straight onto the combined board.
      boardSeries = [...boardSeries, cloneSeries(series)];
      selectedSeriesIndex = boardSeries.length - 1;
      selectedNode = { seriesIndex: selectedSeriesIndex, key: missionKey };
    } else {
      openSingleSeries(series);
    }
  }

  // Cancel add/create on Esc within the board.
  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      if (addingAt != null) addingAt = null;
      else if (creating) creating = false;
    }
  }
</script>

<svelte:window onkeydown={onKeydown} />

<OverlaySurface bind:open title="Missions">
  {#snippet toolbar()}
    {#if inBoard}
      <button class="linkbtn" onclick={backToLanding}>← {boardKind === "country" ? "Countries" : "All series"}</button>
      {#if boardKind === "country"}
        <span class="wtitle">{countryByTag.get(boardTag ?? "")?.name ?? boardTag}</span>
        {#if boardLoading}
          <span class="counter">evaluating…</span>
        {:else if boardSeries}
          <span class="counter">{boardSeries.length} series</span>
          <button class="newbtn" onclick={beginCreate}>＋ New series for {boardTag}</button>
        {/if}
      {:else if boardSeries}
        <code class="wtitle">{boardSeries[0]?.key}</code>
        <span class="badge origin {boardSeries[0]?.origin}">{boardSeries[0]?.origin}</span>
        {#if boardSeries[0]?.pending}<span class="badge pending">unsaved</span>{/if}
      {/if}
    {:else}
      <div class="tabs">
        <button class:on={tab === "country"} onclick={() => (tab = "country")}>By country</button>
        <button class:on={tab === "all"} onclick={() => (tab = "all")}>All series</button>
      </div>
      {#if tab === "all"}
        <input class="search" type="text" placeholder="Search series / file / mission…" bind:value={search} />
      {/if}
      <span class="counter">{allSeries.length} series</span>
    {/if}
  {/snippet}

  <div class="body">
    {#if loading}
      <p class="msg">Loading missions…</p>
    {:else if error}
      <p class="msg err">{error}</p>
    {:else if inBoard}
      <!-- ===== Combined board view ===== -->
      <div class="boardview">
        <div class="boardmain">
          {#if boardLoading}
            <!-- Potentials for the clicked country are still evaluating. -->
            <div class="empty-board">
              <p class="emptymsg">
                Evaluating which series {countryByTag.get(boardTag ?? "")?.name ?? boardTag} receives…
              </p>
            </div>
          {:else if boardSeries && boardSeries.length === 0}
            <!-- Country with no received series → create-from-scratch CTA. -->
            <div class="empty-board">
              <p class="emptymsg">
                {countryByTag.get(boardTag ?? "")?.name ?? boardTag} receives no mission series.
              </p>
              <button class="cta" onclick={beginCreate}>＋ Create mission tree for {boardTag}</button>
            </div>
          {:else if boardSeries}
            <!-- Series settings header (for the selected series) -->
            {#if selSeries}
              <details class="seriescfg" open>
                <summary>Series settings — <code class="cfgkey">{selSeries.key}</code></summary>
                <div class="cfgrow">
                  <label class="cfg">slot
                    <select value={selSeries.slot ?? 1} onchange={(e) => setSlot(Number(e.currentTarget.value))}>
                      {#each [1, 2, 3, 4, 5] as n}<option value={n}>{n}</option>{/each}
                    </select>
                  </label>
                  <label class="cfgchk"><input type="checkbox" checked={selSeries.generic} onchange={() => selSeries && toggleSeriesFlag("generic", selSeries.generic)} /> generic</label>
                  <label class="cfgchk"><input type="checkbox" checked={selSeries.ai} onchange={() => selSeries && toggleSeriesFlag("ai", selSeries.ai)} /> ai</label>
                  <label class="cfgchk"><input type="checkbox" checked={selSeries.hasCountryShield} onchange={() => selSeries && toggleSeriesFlag("has_country_shield", selSeries.hasCountryShield)} /> has_country_shield</label>
                  {#if boardSeries.length > 1}<span class="cfghint">click a series title on the board to edit a different one</span>{/if}
                </div>
                <div class="cfgpot">
                  <h5>Potential <span class="adv">who receives this series</span></h5>
                  {#if potentialBlock}
                    <ScriptTreeEditor file={selSeries.file} rootPath={selSeries.potentialPath} block={potentialBlock}
                      registry="triggers" known={triggers} countries={countryItems} onedit={onTreeEdit} />
                  {:else}
                    <button class="add-block" onclick={addPotential}>＋ Add potential block</button>
                  {/if}
                </div>
              </details>
            {/if}

            {#if boardMessage}<p class="boardmsg">{boardMessage}</p>{/if}

            <MissionBoard
              series={boardSeries}
              {installPath}
              {modPath}
              selectedKey={selectedNode?.key ?? null}
              {selectedSeriesIndex}
              onselect={(i, k) => (selectedNode = { seriesIndex: i, key: k })}
              onselectseries={(i) => (selectedSeriesIndex = i)}
              onmove={moveMission}
              onlink={linkMissions}
              onadd={beginAdd}
            />

            <!-- Approximate series (country tab) -->
            {#if boardKind === "country" && approxSeries.length > 0}
              <details class="approxsec">
                <summary>Possibly received <span class="approx">(approximate — {approxSeries.length})</span></summary>
                <p class="approxnote">These series gate on conditions the evaluator can't fully decide. Add one to the board to edit it (it appears with an APPROX header).</p>
                <ul class="serieslist">
                  {#each approxSeries as s (seriesId(s))}
                    <li><button class="series-card" onclick={() => addApproxSeries(s)}>
                      <span class="skey">{s.key}</span>
                      <span class="smeta">slot {s.slot ?? "?"} · {s.missions.length} missions</span>
                      <span class="badge approxb">approx</span>
                      <span class="addhint">＋ add to board</span>
                    </button></li>
                  {/each}
                </ul>
              </details>
            {/if}
          {/if}
        </div>

        <aside class="boardside">
          {#if addingAt != null}
            <div class="addprompt">
              <h4>Add mission at row {addingAt}</h4>
              <p class="cnote">in series <code>{seriesAt(addingSeriesIndex)?.key}</code></p>
              <input class="addinput" type="text" placeholder="mission_key" bind:value={addKey}
                onkeydown={(e) => e.key === "Enter" && confirmAdd()} />
              {#if addError}<p class="err">{addError}</p>{/if}
              <div class="addbtns">
                <button class="primary" onclick={confirmAdd}>Add</button>
                <button onclick={() => (addingAt = null)}>Cancel</button>
              </div>
            </div>
          {:else if selMission && selNodeSeries && selectedNode}
            <MissionNodeEditor
              mission={selMission}
              series={selNodeSeries}
              {installPath}
              {modPath}
              {queue}
              {triggers}
              {effects}
              countries={countryItems}
              onseticon={(key, icon) => setIcon(selectedNode!.seriesIndex, key, icon)}
              onsetcompletedby={(key, value) => setCompletedBy(selectedNode!.seriesIndex, key, value)}
              onunlink={(dependent, prereq) => unlinkMission(selectedNode!.seriesIndex, dependent, prereq)}
              ondelete={(key) => deleteMission(selectedNode!.seriesIndex, key)}
            />
          {:else}
            <p class="sidehint">Select a mission node to edit it, or click an empty cell to add one. Use <strong>Link requirements</strong> on the board to draw prerequisite arrows — across columns too.</p>
          {/if}
        </aside>
      </div>
    {:else if tab === "country"}
      <!-- ===== Landing: by country (list → straight to board) ===== -->
      <div class="countryview">
        <div class="country-col wide">
          <input class="search" type="text" placeholder="Search countries…" bind:value={search} />
          {#if potLoadingTag}<p class="msg dim">Evaluating {countryByTag.get(potLoadingTag)?.name ?? potLoadingTag}…</p>{/if}
          <div class="country-list">
            {#each countryItems.filter((c) => !search.trim() || c.label.toLowerCase().includes(search.trim().toLowerCase()) || c.key.toLowerCase().includes(search.trim().toLowerCase())) as c (c.key)}
              <button class="country-row" class:sel={c.key === selectedTag} onclick={() => selectCountry(c.key)}>
                <span class="flag" use:lazyFlag={c.key}>
                  {#if flagUrls[c.key]}<img src={flagUrls[c.key]} alt="" />{:else if c.swatch}<span class="sw" style:background={c.swatch}></span>{/if}
                </span>
                <span class="cname">{c.label}</span>
                <code class="ctag">{c.key}</code>
              </button>
            {/each}
          </div>
          <p class="listhint">Click a country to open its combined mission board.</p>
        </div>
      </div>
    {:else}
      <!-- ===== Landing: all series ===== -->
      <div class="allview">
        <button class="newbtn" onclick={beginCreate}>＋ New series</button>
        {#each grouped as g (g.file)}
          <section class="filegroup">
            <header class="filehead">{g.file}<span class="fcount">{g.list.length}</span></header>
            <ul class="serieslist">
              {#each g.list as s (seriesId(s))}
                <li><button class="series-card" onclick={() => openSingleSeries(s)}>
                  <span class="skey">{s.key}</span>
                  <span class="smeta">slot {s.slot ?? "?"} · {s.missions.length} missions</span>
                  <span class="badge origin {s.origin}">{s.origin}</span>
                  {#if s.pending}<span class="badge pending">unsaved</span>{/if}
                </button></li>
              {/each}
            </ul>
          </section>
        {/each}
      </div>
    {/if}

    <!-- New series form (modal-ish inline) -->
    {#if creating}
      <div class="createform">
        <h4>New mission series</h4>
        <label class="cfld">key
          <input type="text" bind:value={newKey} onkeydown={(e) => e.key === "Enter" && confirmCreate()} />
        </label>
        <label class="cfld">slot column
          <select bind:value={newSlot}>{#each [1, 2, 3, 4, 5] as n}<option value={n}>{n}</option>{/each}</select>
        </label>
        <p class="cnote">Potential defaults to {createTag ? `tag = ${createTag}` : "always = yes"} — refine it in Series settings after creating.</p>
        {#if newError}<p class="err">{newError}</p>{/if}
        <div class="addbtns">
          <button class="primary" onclick={confirmCreate}>Create</button>
          <button onclick={() => (creating = false)}>Cancel</button>
        </div>
      </div>
    {/if}
  </div>
</OverlaySurface>

<style>
  .body { display: flex; flex-direction: column; min-height: 0; height: 100%; }

  .tabs { display: flex; gap: 0; }
  .tabs button {
    border: 1px solid var(--border); background: var(--bg-1); color: var(--text-1);
    font-family: inherit; font-size: 0.8rem; padding: 0.2rem 0.7rem; cursor: pointer;
  }
  .tabs button.on { background: var(--accent); color: var(--text-inverse); }

  .search {
    background: var(--bg-1); border: 1px solid var(--border); color: var(--text-1);
    font-family: inherit; font-size: 0.83rem; padding: 0.25rem 0.4rem; width: 16rem;
  }
  .counter, .fcount { font-size: 0.78rem; color: var(--text-2); }
  .linkbtn {
    border: 1px solid var(--border); background: var(--bg-3); color: var(--text-1);
    font-family: inherit; font-size: 0.8rem; padding: 0.2rem 0.6rem; cursor: pointer;
  }
  .linkbtn:hover { background: var(--accent); color: var(--text-inverse); }
  .wtitle { color: var(--ok); background: var(--bg-0); padding: 0.05rem 0.4rem; font-size: 0.82rem; }

  .msg { font-size: 0.85rem; color: var(--text-2); margin: 0.4rem 0; }
  .msg.err { color: var(--err); }
  .msg.dim { color: var(--text-3); }

  /* --- Country view --- */
  .countryview { display: flex; gap: 0.8rem; min-height: 0; height: 100%; }
  .country-col { display: flex; flex-direction: column; gap: 0.4rem; width: 20rem; flex: none; min-height: 0; }
  .country-col.wide { width: 28rem; max-width: 100%; }
  .country-list { overflow-y: auto; border: 1px solid var(--border); display: flex; flex-direction: column; flex: 1; min-height: 0; }
  .country-row {
    display: flex; align-items: center; gap: 0.5rem; border: none; border-bottom: 1px solid var(--border);
    background: transparent; color: var(--text-1); font-family: inherit; font-size: 0.82rem;
    padding: 0.25rem 0.4rem; cursor: pointer; text-align: left;
  }
  .country-row:hover { background: var(--bg-3); }
  .country-row.sel { background: var(--accent); color: var(--text-inverse); }
  .flag { width: 22px; height: 15px; flex: none; display: flex; align-items: center; justify-content: center; }
  .flag img { max-width: 22px; max-height: 15px; }
  .sw { width: 16px; height: 12px; border: 1px solid var(--bg-0); }
  .cname { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .ctag { color: var(--text-2); font-size: 0.72rem; }
  .listhint { font-size: 0.74rem; color: var(--text-3); margin: 0; }

  .approx { color: var(--warn); }

  .serieslist { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 0.25rem; }
  .series-card {
    display: flex; align-items: center; gap: 0.6rem; width: 100%; text-align: left;
    border: 1px solid var(--border); background: var(--bg-1); color: var(--text-1);
    font-family: inherit; font-size: 0.83rem; padding: 0.3rem 0.5rem; cursor: pointer;
  }
  .series-card:hover { background: var(--bg-3); border-color: var(--accent); }
  .skey { font-weight: 600; color: var(--text-1); }
  .smeta { color: var(--text-2); font-size: 0.75rem; }
  .addhint { margin-left: auto; color: var(--accent-text); font-size: 0.72rem; }

  /* --- All view --- */
  .allview { overflow-y: auto; display: flex; flex-direction: column; gap: 0.4rem; }
  .filegroup { border: 1px solid var(--border); }
  .filehead {
    display: flex; justify-content: space-between; background: var(--bg-2); color: var(--ok);
    font-size: 0.8rem; padding: 0.3rem 0.5rem;
  }

  .newbtn {
    align-self: flex-start; border: 1px solid var(--border); background: var(--bg-3); color: var(--text-1);
    font-family: inherit; font-size: 0.8rem; padding: 0.28rem 0.7rem; cursor: pointer;
  }
  .newbtn:hover { background: var(--accent); color: var(--text-inverse); }

  /* --- Board view --- */
  .boardview { display: flex; gap: 0.6rem; min-height: 0; height: 100%; }
  .boardmain { flex: 1; min-width: 0; display: flex; flex-direction: column; min-height: 0; }
  .boardside {
    width: 25rem; flex: none; overflow-y: auto; border-left: 1px solid var(--border);
    padding-left: 0.6rem;
  }
  .sidehint { font-size: 0.82rem; color: var(--text-2); }

  .empty-board {
    display: flex; flex-direction: column; align-items: center; justify-content: center;
    gap: 1rem; height: 100%; text-align: center;
  }
  .emptymsg { font-size: 0.9rem; color: var(--text-2); margin: 0; }
  .cta {
    border: 1px solid var(--accent); background: var(--accent); color: var(--text-inverse);
    font-family: inherit; font-size: 0.95rem; padding: 0.5rem 1.2rem; cursor: pointer;
  }
  .cta:hover { background: var(--accent-text); }

  .seriescfg { border: 1px solid var(--border); margin-bottom: 0.4rem; flex: none; }
  .seriescfg summary { cursor: pointer; padding: 0.3rem 0.5rem; background: var(--bg-2); font-size: 0.82rem; }
  .cfgkey { color: var(--ok); background: var(--bg-0); padding: 0 0.3rem; font-size: 0.78rem; }
  .cfgrow { display: flex; flex-wrap: wrap; gap: 0.8rem; padding: 0.4rem 0.5rem; align-items: center; }
  .cfg { display: flex; align-items: center; gap: 0.3rem; font-size: 0.8rem; color: var(--text-2); }
  .cfg select, .cfld select {
    background: var(--bg-1); border: 1px solid var(--border); color: var(--text-1); font-family: inherit; padding: 0.1rem 0.3rem;
  }
  .cfgchk { display: flex; align-items: center; gap: 0.3rem; font-size: 0.8rem; color: var(--text-1); }
  .cfghint { font-size: 0.72rem; color: var(--text-3); }
  .cfgpot { padding: 0.2rem 0.5rem 0.5rem; border-top: 1px solid var(--border); }
  .cfgpot h5 { margin: 0.3rem 0; font-size: 0.74rem; text-transform: uppercase; letter-spacing: 0.04em; color: var(--text-2); }
  .adv { text-transform: none; letter-spacing: 0; color: var(--text-3); font-size: 0.7rem; }

  .approxsec { border: 1px solid var(--border); margin-top: 0.4rem; flex: none; }
  .approxsec summary { cursor: pointer; padding: 0.3rem 0.5rem; background: var(--bg-2); font-size: 0.8rem; }
  .approxnote { font-size: 0.74rem; color: var(--text-2); margin: 0.3rem 0.5rem; }
  .approxsec .serieslist { padding: 0 0.5rem 0.5rem; }

  .boardmsg { margin: 0.2rem 0; font-size: 0.8rem; color: var(--warn); }

  .add-block {
    align-self: flex-start; border: 1px solid var(--border); background: var(--bg-3); color: var(--text-1);
    font-family: inherit; font-size: 0.78rem; padding: 0.25rem 0.6rem; cursor: pointer;
  }
  .add-block:hover { background: var(--accent); color: var(--text-inverse); }

  .addprompt, .createform { display: flex; flex-direction: column; gap: 0.4rem; }
  .createform {
    position: absolute; top: 3rem; left: 50%; transform: translateX(-50%); z-index: 5;
    background: var(--bg-2); border: 1px solid var(--accent); padding: 0.8rem; width: 22rem;
    box-shadow: 0 8px 30px rgba(0,0,0,0.6);
  }
  .addprompt h4, .createform h4 { margin: 0; font-size: 0.85rem; }
  .addinput, .cfld input {
    background: var(--bg-1); border: 1px solid var(--border); color: var(--text-1);
    font-family: inherit; font-size: 0.84rem; padding: 0.25rem 0.35rem;
  }
  .cfld { display: flex; flex-direction: column; gap: 0.2rem; font-size: 0.78rem; color: var(--text-2); }
  .cnote { font-size: 0.74rem; color: var(--text-2); margin: 0; }
  .addbtns { display: flex; gap: 0.4rem; }
  .addbtns button, .createform button {
    border: 1px solid var(--border); background: var(--bg-3); color: var(--text-1);
    font-family: inherit; font-size: 0.8rem; padding: 0.22rem 0.7rem; cursor: pointer;
  }
  .addbtns button.primary { background: var(--accent); color: var(--text-inverse); }
  .addbtns button:hover { background: var(--accent); color: var(--text-inverse); }
  .err { color: var(--err); font-size: 0.76rem; margin: 0; }

  .badge {
    font-size: 0.64rem; text-transform: uppercase; letter-spacing: 0.03em;
    padding: 0.05rem 0.35rem; border: 1px solid var(--border);
  }
  .badge.origin.base { background: var(--bg-3); color: var(--text-1); }
  .badge.origin.mod { background: var(--ok); color: var(--text-inverse); }
  .badge.pending { background: var(--warn); color: var(--text-inverse); }
  .badge.approxb { background: var(--warn); color: var(--text-inverse); }
</style>
