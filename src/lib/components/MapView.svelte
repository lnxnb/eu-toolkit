<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-dialog";
  import CountryPanel from "./CountryPanel.svelte";
  import type { CountryCreateSeed } from "./country/types";
  import { isCreateCompositeFor } from "./country/delete";
  import ProvincePanel from "./province/ProvincePanel.svelte";
  import ReligionPanel from "./religion/ReligionPanel.svelte";
  import type { ReligionDetails } from "./religion/types";
  import CulturePanel from "./culture/CulturePanel.svelte";
  import { poolBlockValue, type CultureDetails } from "./culture/types";
  import DynastyModal from "./DynastyModal.svelte";
  import WorkshopModal from "./WorkshopModal.svelte";
  import MenuFlyout from "./MenuFlyout.svelte";
  import MapMenuBar from "./MapMenuBar.svelte";
  import { DecisionsOverlay } from "./decisions";
  import { EventsOverlay } from "./events";
  import { MissionsOverlay } from "./missions";
  import { GovernmentNamesOverlay } from "./govnames";
  import { EstatesOverlay } from "./estates";
  import { NewTabView, ShortcutsView, WorkspaceWindow } from "$lib/components/workspace";
  import {
    hasFocusedWorkspaceWindow,
    initializeWorkspace,
    closeTab,
    openView,
    workspaceWindows,
  } from "$lib/workspace.svelte";
  import type { View } from "$lib/views";
  import { requiredMapMode } from "$lib/entityCatalog";
  import { RebelsOverlay } from "./rebels";
  import { MechanicsOverlay } from "./mechanics";
  import { ColorPoolsOverlay } from "./colorpools";
  import { EmpiresOverlay } from "./empires";
  import type { HreMembers } from "$lib/empires";
  import { TechnologyOverlay } from "./technology";
  import { ScriptedOverlay } from "./script";
  import { OnActionsOverlay } from "./onactions";
  import { LocalisationOverlay } from "./loc";
  import { DefinesOverlay } from "./defines";
  import {
    loadScriptedDefs,
    setScriptedJump,
    type ScriptedDef,
  } from "$lib/scripted.svelte";
  import MapBrush from "./MapBrush.svelte";
  import {
    brushDisc,
    provincePixels,
    borderingProvinces,
    provinceColor,
    applyOpsToRgba,
    paintOp,
    dissolveOp,
  } from "$lib/provinceEdit";
  import TradeNetworkOverlay from "./tradenode/TradeNetworkOverlay.svelte";
  import TradeNodePanel from "./tradenode/TradeNodePanel.svelte";
  import AdjacencyOverlay from "./adjacency/AdjacencyOverlay.svelte";
  import AdjacencyPanel from "./adjacency/AdjacencyPanel.svelte";
  import {
    foldAdjacencies,
    rewriteEdit,
    adjacencyAt,
    adjSegments,
    suggestThrough,
    deriveType,
    ADJ_FILE,
    type AdjRow,
    type AdjRowInput,
    type AdjSegment,
  } from "$lib/adjnet";
  import AreaPanel from "./geo/AreaPanel.svelte";
  import RegionPanel from "./geo/RegionPanel.svelte";
  import ColonialPanel from "./geo/ColonialPanel.svelte";
  import IconOverlay from "./IconOverlay.svelte";
  import TradeGoodsList from "./tradegoods/TradeGoodsList.svelte";
  import {
    UNKNOWN_KEY,
    goodKeyOfGroup,
    type TradeGood,
    type TradeGoodsPayload,
    type TradeGoodScaffold,
  } from "./tradegoods/types";
  import type { ValidationIssue, JumpTarget } from "./ValidationStrip.svelte";
  import EditsPanel from "./EditsPanel.svelte";
  import ProblemsOverlay, { type DomainReport } from "./ProblemsOverlay.svelte";
  import SearchOverlay from "./SearchOverlay.svelte";
  import ProjectChangesOverlay from "./ProjectChangesOverlay.svelte";
  import type { SearchRoute } from "$lib/searchRoute";
  import type { EditJump } from "$lib/editsPanel";
  import { BottomToolbar, EmptyState, PromptBanner, InlineNamePrompt, DatePicker, NewGroupModal, createEntityFlow } from "$lib/components/ui";
  import type { ToolButton, EntityFlowState, GroupScaffold, NewGroupResult } from "$lib/components/ui";
  import type { Session } from "$lib/session";
  import { EditQueue, type TypedEdit, type Composite, type BmpOp } from "$lib/edits.svelte";
  import { editAtDate, isShadowed, provinceEditMutations } from "$lib/editAtDate";
  import {
    formatGameDate,
    formatDate,
    eraSuffix,
    effectiveStartDate,
    compareDates,
    MONTH_KEYS,
    WORLD_YEAR_KEY,
    type Calendar,
  } from "$lib/calendar";
  import CalendarEditor from "./CalendarEditor.svelte";
  import { NONE, parseModeData, MapCompositor, REBEL_GRAY, type ModeData, type Rgb, type Override } from "$lib/mapmode";
  import type { War } from "./country/wars";
  import { enemyTags, belligerentTags, hasActiveWar } from "./country/wars";
  import {
    computeCentroids,
    parseAtlasWire,
    type Point,
    type Viewport,
    type Atlas,
    type OverlayItem,
  } from "$lib/overlay";
  import {
    foldNetwork,
    membershipIndex,
    markerAt,
    routeAt,
    handleAt,
    insertIndexAt,
    controlToFileString,
    type TradeNetwork,
    type RouteRef,
    type Xy,
    type DerivedRoute,
  } from "$lib/tradenet";
  import { validateTradeGraph } from "$lib/tradeValidation";
  import {
    foldGeo,
    areaMembershipIndex,
    regionMembershipIndex,
    hashColor as geoHashColor,
    type GeoNetwork,
    type GeoArea,
    type GeoRegion,
  } from "$lib/geonet";
  import {
    foldColonial,
    membershipIndex as colonialMembershipIndex,
    slugify as colonialSlugify,
    uniqueKey as colonialUniqueKey,
    nameLocKey as colonialNameLocKey,
    hashColor as colonialHashColor,
    type ColonialData,
    type ColonialEntry,
  } from "$lib/colonial";
  import {
    collectCircle,
    strokeSamples,
    loadBrushSize,
    saveBrushSize,
    nudgeSize,
    runContinuous,
    type ContinuousTick,
  } from "$lib/brush";
  import DevMixSliders from "./dev/DevMixSliders.svelte";
  import ClimatePanel from "./climate/ClimatePanel.svelte";
  import {
    foldClimate,
    climateKey,
    climateCounts,
    zoneColor,
    winterColor,
    blend,
    type ClimatePayload,
    type ClimateModel,
    type ClimateSlot,
  } from "$lib/climate";
  import SimpleTerrainList from "./terrain/SimpleTerrainList.svelte";
  import {
    AUTO_KEY,
    TERRAIN_FILE,
    foldTerrainEditInto,
    foldTerrainModifiers,
    terrainModifierEdits,
    type EffectiveTerrainPayload,
    type TerrainCategory,
  } from "./terrain/types";
  import type { ModifierRow } from "./ui";
  import {
    newDevAccum,
    tickDevAccum,
    finalizeDevAccum,
    devValue,
    devColor,
    loadDevMix,
    saveDevMix,
    DEV_KEYS,
    DEV_FLOOR,
    type DevMix,
    type DevDir,
    type DevAccum,
  } from "$lib/devpaint";

  let {
    installPath,
    modPath,
    projectName,
    onsession,
    onhome,
  }: {
    installPath: string;
    modPath: string | null;
    projectName: string | null;
    onsession: (session: Session, remount: boolean) => void;
    onhome: () => void;
  } = $props();

  interface MapMode {
    id: string;
    label: string;
    /** Sprint 11.3: terrain/heightmap/province_colors get a "View Only" badge
     *  and offer no tools/panel. */
    viewOnly?: boolean;
  }

  // --- Pending edits (typed queue; one undo unit per composite) ---
  const queue = new EditQueue();
  let saving = $state(false);
  let saveMessage = $state("");
  // Transient non-blocking notice chip (bottom center) — used for soft failures
  // like the create-country tag-collision guard, which must not blank the map.
  let noticeMessage = $state("");
  let noticeTimer: ReturnType<typeof setTimeout> | null = null;
  function notify(msg: string) {
    noticeMessage = msg;
    if (noticeTimer) clearTimeout(noticeTimer);
    noticeTimer = setTimeout(() => (noticeMessage = ""), 4000);
  }
  let dirty = $derived(queue.dirty);

  // --- Menu bar ---
  let openMenu = $state<"file" | "edit" | "view" | "tools" | null>(null);
  // Mass dynasty management (Sprint 1.3), reachable from the Edit menu.
  let dynastiesOpen = $state(false);
  // HRE member province ids to highlight in political mode (null = none).
  let hreHighlightIds = $state<Set<number> | null>(null);
  // Sprint 28 script-plumbing overlays.
  // Fork-from-Steam (18.4) / on-open workshop warn (18.2).
  let steamBacked = $state(false);
  let workshop = $state<{ mode: "browse" | "warn"; source: string | null } | null>(null);
  let workshopWarnPath = $state<string | null>(null);

  // --- Sprint 30.1 Edits panel / 30.2 Problems dashboard ---
  // Composites captured at each successful save this session (grayed history).
  let savedComposites = $state<Composite[]>([]);
  let problemsReports = $state<DomainReport[]>([]);
  let problemsRunning = $state(false);
  let problemsHasRun = $state(false);
  // Live error/warning totals for the View-menu badge (after at least one run).
  const problemsErrorCount = $derived(
    problemsReports.reduce(
      (n, r) => n + r.issues.filter((i) => i.severity === "error").length,
      0,
    ),
  );
  const problemsWarningCount = $derived(
    problemsReports.reduce(
      (n, r) => n + r.issues.filter((i) => i.severity === "warning").length,
      0,
    ),
  );

  /// Runs the aggregate validation command (every domain in one pass) for the
  /// Problems dashboard + the View-menu badge. Date-threaded like `validate`.
  async function runProblems() {
    problemsRunning = true;
    try {
      problemsReports = await invoke<DomainReport[]>("validate_all", {
        installPath,
        modPath,
        date: selectedDate,
      });
      problemsHasRun = true;
    } catch (e) {
      error = String(e);
    } finally {
      problemsRunning = false;
    }
  }

  function openProblems() {
    openMenu = null;
    openView({kind:"problems"});
    // Fetch on first open, or whenever nothing has been run yet.
    if (!problemsHasRun && !problemsRunning) void runProblems();
  }

  /// Routes an Edits-panel jump (best-effort entity/mode switch, Sprint 30.1).
  function editJump(j: EditJump) {
    if (j.kind === "province") openProvince(j.id);
    else if (j.kind === "country") openCountryInPolitical(j.tag);
    else if (j.kind === "mode") setMode(j.mode);
  }

  // Pending selection keys for problems-jump into modes without an existing
  // pending-select path (trade nodes, colonial regions/companies).
  let pendingSelectNodeKey: string | null = null;
  let pendingSelectColonialKey: string | null = null;

  /// Routes a Problems-dashboard jump (typed JumpTarget) to the right mode +
  /// selection, closing the modal so the map is visible (Sprint 30.2).
  function problemsJump(j: JumpTarget) {
    switch (j.kind) {
      case "province":
        openProvince(j.id);
        break;
      case "country":
        openCountryInPolitical(j.id);
        break;
      case "area":
        jumpToAreaMode(j.id);
        break;
      case "node":
        openView({ kind: "trade-node", key: j.id }, "reuse");
        break;
      case "colonial_region":
      case "trade_company": {
        const target = j.kind === "colonial_region" ? "colonial_regions" : "trade_companies";
        openView({ kind: "colonial", colonialKind: target, key: j.id }, "reuse");
        break;
      }
      // "file" targets have no in-map location — nothing to jump to.
    }
  }

  /// Opens a view picked in the New-tab page. Trade nodes, areas/regions,
  /// colonial regions, adjacencies and climate render off mode-scoped state, so
  /// picking one has to switch the map into that mode first; `open` then does
  /// the actual tab navigation (in place, or new window on Shift).
  function openEntityFromPicker(view: View, open: () => void) {
    const need = requiredMapMode(view);
    if (need && need !== mode) {
      if (view.kind === "trade-node") pendingSelectNodeKey = view.key;
      else if (view.kind === "colonial") pendingSelectColonialKey = view.key;
      else if (view.kind === "area") pendingSelectGeoKey = view.key;
      setMode(need);
    }
    open();
  }

  /// Routes a project-wide search hit into its owning editor (Sprint 30.3).
  /// `preview` routes are handled inside the SearchOverlay itself; here we handle
  /// the map-modes / panels / overlays. See src/lib/searchRoute.ts for the table.
  function searchJump(route: SearchRoute) {
    switch (route.kind) {
      case "province":
        openProvince(route.id);
        break;
      case "country":
        openCountryInPolitical(route.tag);
        break;
      case "mode":
        setMode(route.mode);
        break;
      case "overlay":
        switch (route.overlay) {
          case "decisions": openView({kind:"decisions"}); break;
          case "events": openView({kind:"events"}); break;
          case "missions": openView({kind:"missions"}); break;
          case "govnames": openView({kind:"government-names"}); break;
          case "estates": openView({ kind: "estates" }, "reuse"); break;
          case "rebels": openView({kind:"rebels"}); break;
          case "technology": openView({kind:"technology"}); break;
          case "mechanics": openView({kind:"mechanics", family: route.family}); break;
          case "empires": openView({kind:"empires"}); break;
          case "scripted": openView({kind:"scripted"}); break;
          case "onactions": openView({kind:"on-actions"}); break;
          case "localisation": openView({kind:"localisation"}); break;
          case "defines": openView({kind:"defines"}); break;
        }
        break;
    }
  }

  /// Export & Launch (Sprint 30.5): save-guard, then register the mod into
  /// dlc_load.json and boot eu4.exe directly (backend refuses if EU4 is already
  /// running). Requires a saved project; prompts to save when there are pending
  /// edits.
  async function exportAndLaunch() {
    openMenu = null;
    if (!modPath) return;
    if (dirty) {
      if (!confirm("You have unsaved changes. Save the project before launching?")) return;
      await saveProject();
      if (dirty) return; // save was cancelled or failed
    }
    error = "";
    try {
      const plan = await invoke<{ name: string; launched: boolean }>("export_and_launch", {
        installPath,
        modPath,
        dryRun: false,
      });
      if (plan.launched) {
        saveMessage = `Launching EU4 with "${plan.name}"…`;
        setTimeout(() => (saveMessage = ""), 5000);
      }
    } catch (e) {
      error = String(e);
    }
  }

  function confirmDiscard(): boolean {
    return (
      !dirty ||
      confirm("You have unsaved changes. Discard them?")
    );
  }

  async function saveProject() {
    if (!dirty || saving) return;
    openMenu = null;
    let target: string | null = null;
    if (!modPath) {
      const picked = await open({
        directory: true,
        title: "Select or create a folder for your new mod project",
      });
      if (typeof picked !== "string") return;
      target = picked;
    }
    saving = true;
    error = "";
    const savedEdits = queue.serialize();
    // Did this save write calendar loc (month names / era)? If so, re-read the
    // resolved calendar afterwards so the (now-empty) queue's base reflects disk.
    const touchedCalendar = savedEdits.some(
      (e) =>
        e.kind === "locOverride" &&
        ((MONTH_KEYS as readonly string[]).includes(e.key) || e.key === WORLD_YEAR_KEY),
    );
    // Did this save change the province map (add/expand/dissolve, or a new
    // definition row)? If so the id buffer + every mode's render must re-read
    // disk, so we do a full reload after the queue clears.
    const touchedProvinceMap = savedEdits.some(
      (e) =>
        e.kind === "provinceBmp" ||
        (e.kind === "appendText" && e.file === "map/definition.csv"),
    );
    try {
      const written = await invoke<string[]>("save_project", {
        installPath,
        modPath,
        targetDir: target,
        edits: savedEdits,
      });
      // Bake pending political ownership into the in-memory baseline so the map
      // and hit-testing stay correct after the queue clears (edits are now on
      // disk) — no backend re-render.
      if (mode === "political" && compositor && modeData) {
        for (const [id, e] of edited) {
          const gi = e.owner != null ? (tagToGroup.get(e.owner) ?? NONE) : NONE;
          if (id < modeData.values.length) modeData.values[id] = gi;
        }
        // Fold pending map colors into the group metadata so post-save owner
        // repaints use the new color (the pixels are already baked by commit()).
        for (const [t, rgb] of pendingColors) {
          const gi = tagToGroup.get(t);
          if (gi !== undefined && modeData.groups[gi]) modeData.groups[gi].color = rgb;
        }
        compositor.commit();
        pendingColors = new Map();
      }
      // Bake pending religion changes into the in-memory baseline so the map and
      // hit-testing stay correct after the queue clears.
      if (mode === "religion" && compositor && modeData) {
        for (const [id, e] of religionEdited) {
          const gi = e.religion != null ? (religionKeyToGroup.get(e.religion) ?? NONE) : NONE;
          if (id < modeData.values.length) modeData.values[id] = gi;
        }
        for (const [rk, rgb] of pendingReligionColors) {
          const gi = religionKeyToGroup.get(rk);
          if (gi !== undefined && modeData.groups[gi]) modeData.groups[gi].color = rgb;
        }
        compositor.commit();
        pendingReligionColors = new Map();
      }
      // Bake pending culture changes into the in-memory baseline. Display-color
      // overrides (cultureColorOverrides) are persistent toolkit state, not queue
      // edits — they survive the save and are re-applied by refreshPending.
      if (mode === "culture" && compositor && modeData) {
        for (const [id, e] of cultureEdited) {
          const gi = e.culture != null ? (cultureKeyToGroup.get(e.culture) ?? NONE) : NONE;
          if (id < modeData.values.length) modeData.values[id] = gi;
        }
        for (const [ck, rgb] of cultureColorOverrides) {
          const gi = cultureKeyToGroup.get(ck);
          if (gi !== undefined && modeData.groups[gi]) modeData.groups[gi].color = rgb;
        }
        compositor.commit();
      }
      // Bake pending development recolor into the pristine baseline: the gradient
      // pixels already show the painted colors, so committing keeps them after the
      // queue clears (the backend PNG isn't re-rendered). baseProv reloads below,
      // giving the stat overlay the new component values.
      if (mode === "development" && compositor && modeData) {
        compositor.commit();
      }
      // Reload bulk political data from the just-saved project so further
      // painting sees persisted owners/cores (prevents duplicate inserts).
      const projectPath = modPath ?? target;
      try {
        const list = await invoke<ProvincePolitical[]>("get_province_political", {
          installPath,
          modPath: projectPath,
          date: selectedDate,
        });
        const m = new Map<number, ProvincePolitical>();
        for (const p of list) m.set(p.id, p);
        baseProv = m;
      } catch {
        /* keep the pre-save bulk data if the reload fails */
      }
      // Bake pending trade-node changes: the pixels already show the edits, so
      // commit them, reload the network from the saved project, and rebuild
      // mode-data values/colors from that truth so post-clear repaint is stable.
      if (mode === "trade_nodes" && compositor && modeData) {
        compositor.commit();
        try {
          const reloaded = await invoke<TradeNetwork>("get_trade_network", {
            installPath,
            modPath: projectPath,
          });
          baseNetwork = reloaded;
          baseColorPresent = new Set(reloaded.nodes.filter((n) => n.color != null).map((n) => n.key));
          createdNodeKeys = new Set();
          // Ensure every (possibly newly-on-disk) node has a mode-data group.
          for (const n of reloaded.nodes) {
            if (!tradeNodeKeyToGroup.has(n.key)) {
              const idx = modeData.groups.length;
              modeData.groups.push({ key: n.key, label: n.name, color: n.color ?? [128, 128, 128] });
              tradeNodeKeyToGroup.set(n.key, idx);
            } else if (n.color) {
              const gi = tradeNodeKeyToGroup.get(n.key)!;
              if (modeData.groups[gi]) modeData.groups[gi].color = n.color as Rgb;
            }
          }
          // Rebuild province → group from the reloaded membership (full truth).
          const eff = membershipIndex(reloaded);
          for (let id = 0; id < modeData.values.length; id++) {
            const k = eff.get(id) ?? null;
            modeData.values[id] = k != null ? (tradeNodeKeyToGroup.get(k) ?? NONE) : NONE;
          }
          selectedRoute = null;
          // The strip re-derives from the reloaded `baseNetwork` + cleared queue
          // (tradeIssues ⟵ tradeNetwork ⟵ baseNetwork/queue.version); no re-run.
        } catch {
          /* keep the pre-save network if the reload fails */
        }
        // Reload trade-detail overlay data from the saved project so CoT/modifier
        // edits persist into the base overlay after the queue clears (S3.3).
        try {
          const list = await invoke<TradeDetail[]>("get_trade_details", {
            installPath,
            modPath: projectPath,
            date: selectedDate,
          });
          const m = new Map<number, TradeDetail>();
          for (const d of list) m.set(d.id, d);
          tradeDetailBase = m;
          cotEdited = new Map();
          rebuildTradeDetailOverlay();
        } catch {
          /* keep the pre-save trade details if the reload fails */
        }
      }
      // Bake pending area/region changes: reload the geo network from the saved
      // project and rebuild mode-data values from that truth (Sprint 10).
      if ((mode === "areas" || mode === "regions") && compositor && modeData) {
        compositor.commit();
        try {
          const reloaded = await invoke<GeoNetwork>("get_geo_network", {
            installPath,
            modPath: projectPath,
          });
          baseGeo = reloaded;
          const idx = mode === "areas" ? areaMembershipIndex(reloaded) : regionMembershipIndex(reloaded);
          const keyToGroup = mode === "areas" ? areaKeyToGroup : regionKeyToGroup;
          const items: { key: string; name: string; hash: Rgb }[] =
            mode === "areas"
              ? reloaded.areas.map((a) => ({ key: a.key, name: a.name, hash: a.hash_color }))
              : reloaded.regions.map((r) => ({ key: r.key, name: r.name, hash: r.hash_color }));
          for (const it of items) {
            if (!keyToGroup.has(it.key)) {
              const gi = modeData.groups.length;
              modeData.groups.push({ key: it.key, label: it.name, color: it.hash });
              keyToGroup.set(it.key, gi);
            }
          }
          for (let id = 0; id < modeData.values.length; id++) {
            const k = idx.get(id) ?? null;
            modeData.values[id] = k != null ? (keyToGroup.get(k) ?? NONE) : NONE;
          }
          await runGeoValidation();
        } catch {
          /* keep the pre-save network if the reload fails */
        }
      }
      // Bake pending colonial-region / trade-company changes: reload the payload
      // from the saved project and rebuild mode-data values + group colors from
      // that truth (Sprint 19). New entries become base groups; edited colors +
      // membership persist into the base render.
      if (isColonialMode && compositor && modeData) {
        compositor.commit();
        try {
          const reloaded = await invoke<ColonialData>("get_colonial_data", {
            kind: mode,
            installPath,
            modPath: projectPath,
          });
          baseColonial = reloaded;
          for (const e of reloaded.entries) {
            let gi = colonialKeyToGroup.get(e.key);
            if (gi === undefined) {
              gi = modeData.groups.length;
              modeData.groups.push({ key: e.key, label: e.name, color: e.color });
              colonialKeyToGroup.set(e.key, gi);
            } else if (modeData.groups[gi]) {
              modeData.groups[gi].color = e.color;
              modeData.groups[gi].label = e.name;
            }
          }
          const idx = colonialMembershipIndex(reloaded);
          for (let id = 0; id < modeData.values.length; id++) {
            const k = idx.get(id) ?? null;
            modeData.values[id] = k != null ? (colonialKeyToGroup.get(k) ?? NONE) : NONE;
          }
          await runColonialValidation();
        } catch {
          /* keep the pre-save payload if the reload fails */
        }
      }
      // Bake pending trade-good changes into the in-memory baseline, then reload
      // the goods list + icon strip from the saved project so created goods
      // become base entries and their real icons load.
      if (mode === "trade_goods" && compositor && modeData) {
        for (const [id, e] of tradeGoodEdited) {
          const gi = e.good != null ? (tradeGoodKeyToGroup.get(e.good) ?? NONE) : NONE;
          if (id < modeData.values.length) modeData.values[id] = gi;
        }
        for (const [gk, rgb] of pendingTradeGoodColors) {
          const gi = tradeGoodKeyToGroup.get(gk);
          if (gi !== undefined && modeData.groups[gi]) modeData.groups[gi].color = rgb;
        }
        compositor.commit();
        pendingTradeGoodColors = new Map();
        const proj = modPath ?? target;
        try {
          const payload = await invoke<TradeGoodsPayload>("get_trade_goods", {
            installPath,
            modPath: proj,
          });
          baseGoods = payload.goods;
          createdGoods = [];
        } catch {
          /* keep the pre-save goods if the reload fails */
        }
        await loadOverlayAtlas("trade_goods", renderSeq, proj);
      }
      // Bake pending climate changes: the pixels already show the edits, so
      // commit them and reload the two-slot payload from the saved project so
      // post-clear repaint/counts read the new truth (Sprint 11.1).
      if ((mode === "climate" || mode === "winter") && compositor) {
        compositor.commit();
        const proj = modPath ?? target;
        try {
          baseClimate = await invoke<ClimatePayload>("get_climate", { installPath, modPath: proj });
        } catch {
          /* keep the pre-save payload if the reload fails */
        }
      }
      // Bake pending simple-terrain changes: fold the override overlay into the
      // mode-data values (so hit-testing stays right), commit the pixels, and
      // reload the effective-terrain payload from the saved project (Sprint 11.2).
      if (mode === "simple_terrain" && compositor && modeData) {
        for (const [id] of terrainOverlay) {
          const key = terrainEffKey(id);
          const gi = key != null ? (terrainKeyToGroup.get(key) ?? NONE) : NONE;
          if (id < modeData.values.length) modeData.values[id] = gi;
        }
        compositor.commit();
        const proj = modPath ?? target;
        try {
          const payload = await invoke<EffectiveTerrainPayload>("get_effective_terrain", {
            installPath,
            modPath: proj,
          });
          baseTerrain = payload;
          terrainAuto = new Map();
          terrainOverrideBase = new Map();
          for (const p of payload.provinces) {
            if (p.autoTerrain) terrainAuto.set(p.id, p.autoTerrain);
            terrainOverrideBase.set(p.id, p.isOverride);
          }
        } catch {
          /* keep the pre-save payload if the reload fails */
        }
      }
      // Sprint 30.1: capture the just-saved composites into the session "saved"
      // history (grayed, read-only in the Edits panel) before the queue clears.
      savedComposites = [...savedComposites, ...queue.composites];
      queue.clear();
      // Province structural edits (add/expand/dissolve) are now on disk. The
      // session-start id buffer + the current mode's render are stale, so drop
      // them and reload: get_province_ids re-reads the new definition/bitmap and
      // render_map_mode shows the new province in EVERY mode (Province Colors
      // rebuilds its edit canvas from the fresh, now-empty-queue image).
      if (touchedProvinceMap) {
        provinceIds = null;
        pcColorLut = null;
        pcIds = null;
        pcCreated = [];
        pcIdsDirty = true;
        await loadMap();
      }
      // Adjacency edits are now on disk; refresh the base rows so the Provinces
      // overlay stays correct once the (just-cleared) pending edits are gone.
      if (adjLoaded) {
        const savedModPath = modPath ?? target;
        invoke<{ rows: AdjRow[]; waterIds: number[] }>("get_adjacencies", {
          installPath,
          modPath: savedModPath,
        })
          .then((p) => {
            adjBase = p.rows;
            adjWater = new Set(p.waterIds);
            selectedAdjIndex = null;
            runAdjValidation();
          })
          .catch(() => {});
      }
      // Re-read the calendar after a save that renamed months / the era so the
      // base (now on disk) carries the new names — the folded chip/panels stay
      // correct once the pending overrides that produced them are cleared.
      if (touchedCalendar) {
        const savedModPath = modPath ?? target;
        invoke<CalendarLoc>("get_calendar_loc", { installPath, modPath: savedModPath })
          .then((cal) => {
            baseCalendarMonths = cal.months;
            baseWorldYear = cal.worldYear;
          })
          .catch(() => {});
      }
      // Created countries are now on disk; drop their pending scaffold seeds so
      // the panel loads real details for them.
      if (countryScaffoldSeeds.size > 0) countryScaffoldSeeds = new Map();
      saveMessage = `Saved ${written.length} file${written.length === 1 ? "" : "s"}`;
      setTimeout(() => (saveMessage = ""), 3000);
      // Sprint 30.2: auto re-run the Problems dashboard against the just-saved
      // state so its badge + list reflect what actually landed on disk.
      if (problemsHasRun) void runProblems();
      // Sprint 28: refresh the scripted-name registry so a newly-saved scripted
      // trigger/effect resolves as a link in every 14.2 tree.
      void loadScriptedDefs(installPath, modPath);
      if (!modPath && target) {
        const name = await invoke<string>("validate_project", { path: target });
        // Session becomes a project session; no remount (map is unchanged).
        onsession({ installPath, modPath: target, projectName: name }, false);
      }
    } catch (e) {
      error = String(e);
    }
    saving = false;
  }

  function doUndo() {
    openMenu = null;
    queue.undo();
  }

  function doRedo() {
    openMenu = null;
    queue.redo();
  }

  async function exportToGame() {
    openMenu = null;
    if (!modPath) return;
    error = "";
    try {
      const name = await invoke<string>("export_to_game", {
        installPath,
        modPath,
      });
      saveMessage = `"${name}" registered with the game launcher`;
      setTimeout(() => (saveMessage = ""), 4000);
    } catch (e) {
      error = String(e);
    }
  }

  async function menuOpenProject() {
    openMenu = null;
    if (!confirmDiscard()) return;
    const path = await open({
      directory: true,
      title: "Select an EU4 mod project folder",
    });
    if (typeof path !== "string") return;
    // Workshop mods sit in a Steam-managed folder overwritten on updates —
    // warn (don't block) and offer to fork.
    try {
      if (await invoke<boolean>("is_workshop_path", { path })) {
        workshopWarnPath = path;
        workshop = { mode: "warn", source: path };
        return;
      }
    } catch {
      /* best-effort detection */
    }
    try {
      const name = await invoke<string>("validate_project", { path });
      onsession({ installPath, modPath: path, projectName: name }, true);
    } catch (e) {
      error = String(e);
    }
  }

  function menuForkFromSteam() {
    openMenu = null;
    if (!steamBacked) return;
    if (!confirmDiscard()) return;
    workshop = { mode: "browse", source: null };
  }

  function onWorkshopForked(fork: { path: string; name: string }) {
    workshop = null;
    workshopWarnPath = null;
    onsession({ installPath, modPath: fork.path, projectName: fork.name }, true);
  }

  async function openWorkshopAnyway() {
    const path = workshopWarnPath;
    workshop = null;
    workshopWarnPath = null;
    if (!path) return;
    try {
      const name = await invoke<string>("validate_project", { path });
      onsession({ installPath, modPath: path, projectName: name }, true);
    } catch (e) {
      error = String(e);
    }
  }

  // --- Rename the project from the title bar -------------------------------
  // Writes the descriptor's `name` immediately (it is not a game-data edit, so
  // it does not join the pending queue); the session keeps its identity, so the
  // shell updates the label without a remount.
  let renaming = $state(false);
  let renameValue = $state("");
  let renameInput = $state<HTMLInputElement | null>(null);

  function startRename() {
    if (!modPath) return;
    renameValue = projectName ?? "";
    renaming = true;
  }

  $effect(() => {
    if (renaming && renameInput) renameInput.select();
  });

  async function commitRename() {
    if (!renaming) return;
    const next = renameValue.trim();
    renaming = false;
    if (!modPath || !next || next === projectName) return;
    try {
      const name = await invoke<string>("rename_project", { modPath, name: next });
      onsession({ installPath, modPath, projectName: name }, false);
    } catch (e) {
      error = String(e);
    }
  }

  function renameKey(e: KeyboardEvent) {
    // Keep Ctrl+S / Ctrl+Shift+F (window-level) from firing while typing.
    e.stopPropagation();
    if (e.key === "Enter") {
      e.preventDefault();
      void commitRename();
    } else if (e.key === "Escape") {
      e.preventDefault();
      renaming = false;
    }
  }

  function menuOpenBase() {
    openMenu = null;
    if (!confirmDiscard()) return;
    onsession({ installPath, modPath: null, projectName: null }, true);
  }

  // New Project ▸ Start from base game: a fresh base session (discarding any
  // pending edits), the launch screen's "Start from Base Game" reachable in-app.
  function menuNewFromBase() {
    openMenu = null;
    if (!confirmDiscard()) return;
    onsession({ installPath, modPath: null, projectName: null }, true);
  }

  // New Project ▸ Start from blank: scaffold a mod that keeps the base map and
  // definitions but empties the world (SPRINT2 18.3), then open it. The session
  // remounts and the project is recorded into recents via the +page funnel.
  async function menuNewFromBlank() {
    openMenu = null;
    if (!confirmDiscard()) return;
    const target = await open({
      directory: true,
      title: "Select or create an empty folder for your blank-world mod project",
    });
    if (typeof target !== "string") return;
    error = "";
    try {
      const name = await invoke<string>("scaffold_blank_project", {
        installPath,
        targetDir: target,
      });
      onsession({ installPath, modPath: target, projectName: name }, true);
    } catch (e) {
      error = String(e);
    }
  }

  function menuHome() {
    openMenu = null;
    if (!confirmDiscard()) return;
    onhome();
  }

  /// Centres the viewport on a province (the country panel's zoom-to-capital
  /// flag). Zooms in to at least `LOCATE_SCALE` — a province is a few pixels at
  /// the fitted scale — but never zooms back OUT of a closer view.
  const LOCATE_SCALE = 6;

  function centerOnProvince(id: number) {
    if (!bitmap || !container || !provinceIds) return;
    ensureCentroids();
    const p = centroids.get(id);
    if (!p) return;
    scale = Math.min(MAX_SCALE, Math.max(scale, LOCATE_SCALE));
    offsetX = container.clientWidth / 2 - p.x * scale;
    offsetY = container.clientHeight / 2 - p.y * scale;
    redraw();
  }

  function zoomTo100() {
    openMenu = null;
    if (!bitmap || !container) return;
    const f = 1 / scale;
    const cx = container.clientWidth / 2;
    const cy = container.clientHeight / 2;
    offsetX = cx - (cx - offsetX) * f;
    offsetY = cy - (cy - offsetY) * f;
    scale = 1;
    redraw();
  }

  let container: HTMLDivElement;
  let canvas: HTMLCanvasElement;

  let modes = $state<MapMode[]>([]);
  let mode = $state("provinces");
  let loading = $state(true);
  let error = $state("");
  let zoomPct = $state(100);

  // --- Date selector (Sprint 12.1/12.2) ---
  interface Bookmark {
    file: string;
    nameKey: string;
    name: string;
    descKey: string;
    desc: string;
    date: string;
    isDefault: boolean;
    center: number | null;
    countries: string[];
  }
  interface CalendarLoc {
    months: string[];
    worldYear: string | null;
  }
  interface DefinesDates {
    startDate: string;
    endDate: string;
  }
  interface BookmarkScaffold {
    file: string;
    text: string;
    nameKey: string;
    descKey: string;
    outOfRange: boolean;
    rangeStart: string;
    rangeEnd: string;
  }
  // The date every date-aware command is derived at. null until resolved on
  // mount; once set it is always an explicit "Y.M.D" so panels re-fetch on change.
  let selectedDate = $state<string | null>(null);
  // The mod's effective start (default bookmark, else earliest, else vanilla),
  // used as the "static files aren't date-aware" comparison baseline + default.
  let effectiveStart = $state("1444.11.11");
  let bookmarks = $state<Bookmark[]>([]);
  // Resolved calendar loc as read from the backend (base + mod). The displayed
  // month names / era fold pending locOverride edits on top (12.4) so a rename in
  // the calendar editor shows immediately in the chip and every rendered date.
  let baseCalendarMonths = $state<string[]>([]);
  let baseWorldYear = $state<string | null>(null);
  let calendarMonths = $derived.by<string[]>(() => {
    queue.version;
    return MONTH_KEYS.map(
      (k, i) => queue.pendingLocOverride(k) ?? baseCalendarMonths[i] ?? String(i + 1),
    );
  });
  let worldYearTemplate = $derived.by<string | null>(() => {
    queue.version;
    return queue.pendingLocOverride(WORLD_YEAR_KEY) ?? baseWorldYear;
  });
  let eraSuffixStr = $derived(eraSuffix(worldYearTemplate));
  // The shared calendar handed to date-rendering panels (timeline, diplomacy).
  let calendar = $derived<Calendar>({ months: calendarMonths, era: eraSuffixStr });
  let definesDates = $state<DefinesDates | null>(null);
  let dateMenuOpen = $state(false);
  // The "Edit calendar…" sub-panel is open (swaps the bookmark list).
  let calendarEditOpen = $state(false);
  // Busy while the whole view re-derives after a date change (chip spinner).
  let dateBusy = $state(false);
  // "+ New start date…" inline form state.
  let newDateOpen = $state(false);
  let newDateValue = $state("1444.11.11");
  let newDateName = $state("");
  let newDateError = $state("");

  // The chip label = the selected date rendered with the mod's calendar.
  let dateLabel = $derived(
    selectedDate ? formatGameDate(selectedDate, calendarMonths, eraSuffixStr) : "…",
  );
  // A date differing from the effective start means static-file modes (which are
  // NOT date-aware in EU4) show a note that they ignore the selection.
  let dateIsStart = $derived(
    selectedDate == null || compareDates(selectedDate, effectiveStart) === 0,
  );
  const STATIC_MODES = new Set([
    "areas",
    "regions",
    "climate",
    "winter",
    "simple_terrain",
    "trade_nodes",
  ]);
  let showStaticNote = $derived(!dateIsStart && STATIC_MODES.has(mode));
  // Bookmarks for the dropdown, sorted by date (optimistic new ones re-sort in).
  let sortedBookmarks = $derived(
    [...bookmarks].sort(
      (a, b) => compareDates(a.date, b.date) || a.name.localeCompare(b.name),
    ),
  );

  let bitmap = $state<ImageBitmap | null>(null);
  let scale = 1;
  let minScale = 0.05;
  const MAX_SCALE = 40;
  let offsetX = 0;
  let offsetY = 0;
  let dragging = $state(false);
  let lastX = 0;
  let lastY = 0;
  let downX = 0;
  let downY = 0;
  let moved = 0;

  // --- Unified mode-data hit-testing & hover/selection ---
  // Per-pixel province id buffer (loaded once, reused across modes). $state so the
  // trade-goods icon overlay (7.6) reacts when the buffer first loads.
  let mapW = $state(0);
  let mapH = $state(0);
  let provinceIds = $state<Uint16Array | null>(null);
  // Current mode's group/value data (categorical modes drive hover/selection).
  let modeData = $state<ModeData | null>(null);
  // Compositing pipeline for the current categorical mode (null otherwise).
  let compositor: MapCompositor | null = null;
  // Province-id-indexed darken table, reused between highlight rebuilds.
  const provinceFill = new Uint32Array(65536);
  const HOVER_FILL = 0x3c000000; // semi-transparent black (RGBA little-endian)
  const SELECTED_FILL = 0x5a000000;
  // Brush preview tint (bluish, RGBA little-endian): provinces the armed brush
  // WOULD affect on click, distinct from the hover/selection darken.
  const PREVIEW_FILL = 0x55a76d4a;
  // Trade-node route path tint (goldish) + unassigned-province tint (magenta),
  // both packed little-endian RGBA.
  const ROUTE_FILL = 0x5540c8ff;
  const UNASSIGNED_FILL = 0x3cc040c0;
  // HRE-member highlight (Sprint 29): translucent gold over Empire provinces.
  const HRE_FILL = 0x5522ccff;
  // Uncolonized-land fill in the political render (matches map_renderer LAND).
  const LAND_RGB: Rgb = [200, 200, 196];

  let hoverGroup = $state<number>(NONE);
  let selectedGroup = $state<number>(NONE);
  let hovering = $state(false);
  // Province under the cursor in political mode (Sprint 13.3 occupation footer).
  let politicalHoverId = $state<number>(NONE);

  // --- Bottom toolbar + brush (Phase 0.5 / Sprint 1.4) ---
  let armedTool = $state<string | null>(null);
  let brushSize = $state(loadBrushSize());
  // Circle-outline cursor (screen coords + on-screen diameter px) while a
  // brush is armed. `d` is precomputed so the template needn't read `scale`.
  let brushCursor = $state<{ x: number; y: number; on: boolean; d: number }>({
    x: 0,
    y: 0,
    on: false,
    d: 4,
  });

  function setBrushCursor(x: number, y: number, on: boolean) {
    brushCursor = { x, y, on, d: Math.max(4, brushSize * scale) };
  }

  $effect(() => saveBrushSize(brushSize));

  // --- Province Colors structural editing (add / expand / dissolve) --------
  // Its own edit model: color-space pixel ops on provinces.bmp (backend
  // province_edit), NOT the per-province override brushes above. `pc_*` tool ids
  // stay out of BRUSH_TOOLS so none of the categorical paint machinery engages.
  const PC_TOOLS = new Set(["pc_new", "pc_expand", "pc_dissolve"]);
  let pcArmed = $derived(PC_TOOLS.has(armedTool ?? ""));
  // New/Expand paint with the disc brush; Dissolve is a plain click (no circle).
  let pcBrushArmed = $derived(armedTool === "pc_new" || armedTool === "pc_expand");
  // Edit-aware province-id buffer. The session-start `provinceIds` doesn't know
  // about pending carves/expands/dissolves, so hit-testing a province you just
  // drew would hit the one underneath. `pcColorLut` maps a bitmap color → id
  // (existing provinces from the pristine scan + provinces created this session);
  // `pcIds` is rebuilt from the CURRENT edited colors, so clicks resolve to the
  // right province. Freed on leaving the mode (it is a ~33 MB lookup table).
  let pcColorLut: Uint16Array | null = null; // packed-rgb (2^24) → province id
  let pcIds: Uint16Array | null = null; // edited province-id-per-pixel
  let pcIdsDirty = true;
  let pcCreated: { color: Rgb; id: number }[] = []; // provinces carved this session
  // The displayed, mutable province_colors bitmap = saved image + pending ops.
  let pcEditCanvas: HTMLCanvasElement | null = null;
  let pcPristine: ImageData | null = null; // the saved (rendered) province_colors image
  let pcCurImage: ImageData | null = null; // current colors (pristine + pending) backing the canvas
  let pcOverlay: HTMLCanvasElement | null = null; // source/target tint highlight
  // Selection: Expand's grow-target, or Dissolve's source province.
  let pcSelectedId = $state<number | null>(null);
  let pcTargets = $state<Set<number>>(new Set()); // Dissolve target neighbours
  // Active brush stroke (Expand paint / New carve).
  let pcPointerActive = false; // a left-press on the map with a pc tool armed
  let pcMoved = 0;
  let pcDownX = 0;
  let pcDownY = 0;
  let pcPainting = false; // the press turned into a drag → painting
  let pcStrokePixels: Set<number> = new Set();
  let pcStrokeColor: Rgb | null = null; // the color the active stroke paints
  // New-province name prompt (shown after a New carve stroke).
  let pcNamePrompt = $state<{ x: number; y: number; pixels: number[]; sourceId: number } | null>(
    null,
  );
  let pcNewName = $state("New Province");
  let pcDissolveCandidates: Set<number> = new Set(); // provinces a Dissolve may divide into
  let pcSourceId = 0; // province under a New carve's first pixel (inherits area/culture)

  // Bulk per-province political + eligibility data (loaded once per session).
  interface ProvincePolitical {
    id: number;
    file: string;
    owner: string | null;
    controller: string | null;
    cores: string[];
    water: boolean;
    wasteland: boolean;
    // Dev components (Sprint 9); null = key absent (created at the floor on paint).
    base_tax: number | null;
    base_production: number | null;
    base_manpower: number | null;
  }
  let baseProv: Map<number, ProvincePolitical> | null = null;

  // Effective pending political state per touched province = base + queue
  // (+ the in-progress stroke). The single source of truth for repaint,
  // hit-testing, and insert-vs-replace edit generation.
  interface Eff {
    owner: string | null;
    ownerPresent: boolean;
    /** Effective controller tag (occupation); null = no controller key / uncolonized. */
    controller: string | null;
    controllerPresent: boolean;
    cores: Set<string>;
  }
  let edited = new Map<number, Eff>();
  // Province id -> pending group index (or NONE) overlaying modeData.values.
  let pendingGroup = new Map<number, number>();
  // Tag -> political group index, rebuilt when political mode data loads.
  let tagToGroup = new Map<string, number>();
  // Tag -> pending map color (from the country panel's ColorPicker). Repaints
  // every province of that country live, before save.
  let pendingColors = new Map<string, Rgb>();
  // Province id chosen by the Set Capital tool, handed to the open CountryPanel.
  let capitalRequest = $state<number | null>(null);
  // Province id chosen by the province-names "Pick on map" tool (Sprint 24),
  // handed to whichever province-names section armed it (culture/group/country).
  let provNamePick = $state<number | null>(null);

  // --- Create country (Sprint 4.1) ----------------------------------------
  // The project-owned tag registration file the scaffold appends to; parsing it
  // out of the queue gives the pending-created tags (undo/redo aware).
  const COUNTRY_TAG_FILE = "common/country_tags/zz_eutoolkit_countries.txt";
  // Full create-country payload from prepare_country_scaffold (snake_case).
  interface CountryScaffold {
    tag: string;
    color: [number, number, number];
    capital_id: number;
    name: string;
    adjective: string;
    country_file: string;
    history_file: string;
    flag_file: string;
    edits: TypedEdit[];
  }
  // Tag -> display seed for a pending-created country (handed to CountryPanel,
  // which can't fetch details until save). Reactive so the panel re-reads it.
  let countryScaffoldSeeds = $state(new Map<string, CountryCreateSeed>());
  // Bumped on a water/wasteland create click to shake the prompt banner.
  let createShakeKey = $state(0);
  // Tags with a pending (unsaved) tag-registration in the queue — derived so
  // undo/redo keep it honest. Guards a save-less second create colliding a tag.
  let pendingCreatedTags = $derived.by<Set<string>>(() => {
    queue.version;
    const s = new Set<string>();
    for (const e of queue.serialize()) {
      if (e.kind === "appendText" && e.file === COUNTRY_TAG_FILE) {
        const m = e.text.match(/([A-Z0-9]{3})\s*=/);
        if (m) s.add(m[1]);
      }
    }
    return s;
  });

  // --- Religion mode pending projection (Sprint 5) ---
  // Religion key -> mode-data group index, rebuilt when religion mode loads.
  let religionKeyToGroup = new Map<string, number>();
  // Religion key -> pending map color (panel ColorPicker). Recolors every
  // province of that religion live, before save.
  let pendingReligionColors = new Map<string, Rgb>();
  // Effective pending religion per touched province = base (mode-data) + queue
  // (+ active stroke). Drives repaint, hit-testing, and insert-vs-set edits.
  interface RelEff {
    religion: string | null;
    present: boolean;
  }
  let religionEdited = new Map<number, RelEff>();
  // Seed for a just-created religion not yet on disk (handed to the panel).
  let createdSeed = $state<ReligionDetails | null>(null);

  // --- Culture mode pending projection (Sprint 6) ---
  // Culture key -> mode-data group index, rebuilt when culture mode loads.
  let cultureKeyToGroup = new Map<string, number>();
  // Culture key -> display-color override (toolkit DB, not the mod). Pre-loaded
  // on mode load and updated live by the panel; recolors every province of that
  // culture. Persistent (survives save) — cleared only by the panel's Reset.
  let cultureColorOverrides = new Map<string, Rgb>();
  // Effective pending culture per touched province = base (mode-data) + queue
  // (+ active stroke). Drives repaint, hit-testing, and insert-vs-set edits.
  interface CulEff {
    culture: string | null;
    present: boolean;
  }
  let cultureEdited = new Map<number, CulEff>();
  // Seed for a just-created culture not yet on disk (handed to the panel).
  let createdCultureSeed = $state<CultureDetails | null>(null);

  // --- Trade Nodes mode pending projection (Sprint 8) -----------------------
  // Base network payload (get_trade_network); the *effective* network folds the
  // queue over it (see `tradeNetwork`). Node key -> mode-data group index.
  const TRADE_NODE_FILE = "common/tradenodes/zz_eutoolkit_tradenodes.txt";
  let baseNetwork = $state<TradeNetwork | null>(null);
  let tradeNodeKeyToGroup = new Map<string, number>();
  // Node keys with a `color` on disk (or in a create scaffold) — SetBlock vs
  // InsertStatement, and the panel's colorPresent flag.
  let baseColorPresent = $state(new Set<string>());
  let createdNodeKeys = $state(new Set<string>());
  // province id -> current effective node key (recolor + steal eligibility).
  let tnMemberCache = new Map<number, string>();
  // Live membership overlay for the in-progress paint stroke (id -> key|null).
  let tnPaintOverride = new Map<number, string | null>();
  let selectedRoute = $state<RouteRef | null>(null);
  let hoverNode = $state<string | null>(null);
  let hoverRoute = $state<RouteRef | null>(null);
  // Live control points (top-left) of the route under drag; overrides the stored.
  let editControl = $state<Xy[] | null>(null);
  let draggingHandle = -1;
  let showUnassigned = $state(false);
  // Province centroids (shared with the overlay + marker hit-testing).
  let centroids = $state(new Map<number, Point>());
  let centroidsSource: Uint16Array | null = null;

  // --- Provinces mode: straits & adjacencies (Sprint 25) --------------------
  // Base rows from get_adjacencies (index = file order); the effective list
  // folds the queue's csvRewrite edits over it. Static (no date threading).
  interface AdjIssue {
    severity: string;
    message: string;
    row: number;
  }
  let adjBase = $state<AdjRow[]>([]);
  let adjWater = $state<Set<number>>(new Set());
  let adjLoaded = false;
  let selectedAdjIndex = $state<number | null>(null);
  let hoverAdjIndex = $state<number | null>(null);
  let adjIssues = $state<AdjIssue[]>([]);
  // First endpoint captured while the "+ Add strait" tool collects two clicks
  // ($state so the add-strait prompt updates between the first and second click).
  let adjAddFirst = $state<number | null>(null);
  const effectiveAdj = $derived.by<AdjRowInput[]>(() => {
    void queue.version;
    return foldAdjacencies(adjBase, queue.serialize());
  });
  const selectedAdj = $derived(
    selectedAdjIndex != null ? (effectiveAdj[selectedAdjIndex] ?? null) : null,
  );
  // Drawable line segments, trimmed to the edge-to-edge crossing via the
  // province-id buffer (view-independent; recomputed only when the rows,
  // centroids, or id buffer change). Shared by the overlay and hit-testing.
  const adjSegs = $derived.by<(AdjSegment | null)[]>(() => {
    const ids = provinceIds;
    const idAt = ids
      ? (x: number, y: number) =>
          x >= 0 && y >= 0 && x < mapW && y < mapH ? ids[y * mapW + x] : NONE
      : null;
    return adjSegments(effectiveAdj, centroids, idAt, mapW);
  });
  const selectedAdjIssues = $derived(
    adjIssues.filter((i) => i.row === selectedAdjIndex),
  );

  // --- Trade-detail overlay (S3.3): CoT icons + trade-modifier badges ---------
  interface TradeModifierRef {
    key: string;
    name: string;
  }
  interface TradeDetail {
    id: number;
    cot: number | null;
    coastal: boolean;
    modifiers: TradeModifierRef[];
  }
  // Base per-province trade detail from get_trade_details (date-folded backend).
  let tradeDetailBase = new Map<number, TradeDetail>();
  // Pending CoT overrides folded from the queue (province id → tier | null),
  // so a province-panel CoT edit updates the overlay live.
  let cotEdited = new Map<number, number | null>();
  // View toggle (persisted); the overlay only renders in trade_nodes mode.
  let showTradeDetails = $state(true);
  // Guards persisting the default before the stored value loads on mount.
  let tradeDetailsLoaded = false;
  // Persist the toggle and rebuild the overlay when it flips.
  $effect(() => {
    const on = showTradeDetails;
    if (tradeDetailsLoaded) {
      invoke("set_view_toggle", { key: "trade_details", value: on }).catch(() => {});
    }
    if (mode === "trade_nodes") rebuildTradeDetailOverlay();
  });
  // Hover tooltip listing a province's trade-modifier display names.
  let tradeDetailTooltip = $state<{ x: number; y: number; names: string[] } | null>(null);

  // View toggle (persisted) for the Provinces-mode adjacency lines. Purely a
  // rendering/hit-testing gate — the rows themselves are untouched.
  let showStraits = $state(true);
  let straitsLoaded = false;
  $effect(() => {
    const on = showStraits;
    if (straitsLoaded) invoke("set_view_toggle", { key: "straits", value: on }).catch(() => {});
  });

  // Reactive viewport + container size for the trade-node overlay (mirrors the
  // live map transform; updated in redraw()/resize()).
  let view = $state<Viewport>({ scale: 1, offsetX: 0, offsetY: 0 });
  let cssW = $state(0);
  let cssH = $state(0);

  // The effective trade network = base + PENDING (folds the whole queue). Drives
  // the overlay, the node panel, and the map recolor; undo/redo revert it.
  let tradeNetwork = $derived.by<TradeNetwork | null>(() => {
    queue.version;
    return baseNetwork ? foldNetwork(baseNetwork, queue.serialize()) : null;
  });

  // S2.8 — the trade-graph validation strip runs CLIENT-SIDE over the effective
  // (folded) network, not the backend `validate` command over saved state. This
  // makes cycles / unreachable ends / orphans flag the instant an edit is queued
  // (and clear on undo): both `tradeNetwork` and this depend on `queue.version`,
  // so `queue.push` / `undo` / `redo` (each bump `#rev`) recompute the whole
  // chain → the strip re-renders with no per-pointer work and no IPC. On an empty
  // queue the effective network equals the saved one, so this matches what the
  // backend trade_nodes domain would report. Cheap (~80 nodes) — no debounce.
  let tradeIssues = $derived<ValidationIssue[]>(
    tradeNetwork ? validateTradeGraph(tradeNetwork) : [],
  );

  // --- Areas / Regions mode pending projection (Sprint 10) ------------------
  // Base geo network (get_geo_network); the effective one folds the queue over
  // it. Areas contain provinces; regions contain areas. key -> mode-data group.
  let baseGeo = $state<GeoNetwork | null>(null);
  let areaKeyToGroup = new Map<string, number>();
  let regionKeyToGroup = new Map<string, number>();
  // province id -> current effective area/region key (recolor + steal).
  let geoMemberCache = new Map<number, string>();
  // Live membership overlay for the in-progress paint stroke (id -> key|null).
  let geoPaintOverride = new Map<number, string | null>();
  let geoIssues = $state<ValidationIssue[]>([]);

  // Effective geo network = base + PENDING (folds the whole queue).
  let geoNetwork = $derived.by<GeoNetwork | null>(() => {
    queue.version;
    return baseGeo ? foldGeo(baseGeo, queue.serialize()) : null;
  });

  // --- Colonial Regions / Trade Companies mode (Sprint 19) ------------------
  // Same province-membership machinery as areas, but entries carry an explicit
  // color and a rich panel. `baseColonial` is the get_colonial_data payload for
  // the active mode's kind; the effective one folds the queue.
  let baseColonial = $state<ColonialData | null>(null);
  let colonialKeyToGroup = new Map<string, number>();
  // province id -> current effective colonial entry key (recolor + steal).
  let colonialMemberCache = new Map<number, string>();
  // HRE membership brush (Sprint 29). `hreMemberCache` = provinces with hre=yes at
  // the selected date (from get_hre_members); `hreEdited` overlays pending toggles
  // so the brush dedupes flips and the preview reflects the queue.
  let hreMemberCache = new Set<number>();
  let hreEdited = new Map<number, boolean>();
  let colonialIssues = $state<ValidationIssue[]>([]);
  let isColonialMode = $derived(mode === "colonial_regions" || mode === "trade_companies");
  let colonialData = $derived.by<ColonialData | null>(() => {
    queue.version;
    return baseColonial ? foldColonial(baseColonial, queue.serialize()) : null;
  });

  // --- Trade Goods mode pending projection (Sprint 7) -----------------------
  // Good key -> mode-data group index, rebuilt when trade_goods mode loads.
  const GOODS_FILE = "common/tradegoods/zz_eutoolkit_tradegoods.txt";
  const PRICES_FILE = "common/prices/zz_eutoolkit_prices.txt";
  let tradeGoodKeyToGroup = new Map<string, number>();
  // The base goods payload (get_trade_goods) + goods created (unsaved) this
  // session; the effective list appends the latter in definition order (7.4).
  let baseGoods = $state<TradeGood[]>([]);
  let createdGoods = $state<TradeGood[]>([]);
  let allGoods = $derived<TradeGood[]>([...baseGoods, ...createdGoods]);
  // Good key -> province count (base mode-data + PENDING fold). Reactive to the
  // queue so counts update on paint/undo/redo; folds the whole queue itself so it
  // doesn't depend on the imperatively-rebuilt `tradeGoodEdited`.
  let goodCounts = $derived.by<Map<string, number>>(() => {
    queue.version;
    const m = new Map<string, number>();
    if (mode !== "trade_goods" || !modeData) return m;
    const overlay = new Map<number, string | null>();
    for (const e of queue.serialize()) foldTradeGoodEditInto(overlay, e);
    const vals = modeData.values;
    for (let id = 0; id < vals.length; id++) {
      const raw = overlay.has(id)
        ? overlay.get(id)
        : vals[id] !== NONE
          ? (modeData.groups[vals[id]]?.key ?? null)
          : null;
      // Cluster groups count toward their base good ("unknown").
      const key = raw != null ? goodKeyOfGroup(raw) : null;
      if (key) m.set(key, (m.get(key) ?? 0) + 1);
    }
    return m;
  });
  // The good selected in the list — the paint target. May be a real good, the
  // "unknown" sentinel, or a zero-province good that has no mode-data group.
  let selectedGoodKey = $state<string | null>(null);
  // Good key -> pending map color (from the editor's ColorPicker) — live recolor.
  let pendingTradeGoodColors = new Map<string, Rgb>();
  // Effective pending trade good per touched province = base (mode-data) + queue
  // (+ active stroke). Drives repaint, hit-testing, and insert-vs-set edits.
  interface GoodEff {
    good: string | null;
    present: boolean;
  }
  let tradeGoodEdited = new Map<number, GoodEff>();
  // Reusable icon-overlay wiring (Sprint 7.6). A small mode → config map keeps it
  // generic — religion icons later ride the same layer by adding an entry here.
  const ICON_OVERLAY_MODES: Record<string, { atlasKind: string; skip: Set<string> }> = {
    trade_goods: { atlasKind: "trade_goods", skip: new Set([UNKNOWN_KEY]) },
    // Development stat boxes (9.3) ride the same overlay layer; the atlas supplies
    // the tax/production/manpower icons drawn beside each number.
    development: { atlasKind: "development", skip: new Set() },
    // Trade-detail overlay (S3.3): CoT tier icons + trade-modifier badges, drawn
    // over the trade-node network. Gated additionally by `showTradeDetails`.
    trade_nodes: { atlasKind: "trade_details", skip: new Set() },
  };
  let overlayAtlas = $state<Atlas | null>(null);
  let overlayAtlasIndex = $state(new Map<string, number>());
  let overlayItems = $state<Map<number, OverlayItem>>(new Map());

  // --- Development mode (Sprint 9) ------------------------------------------
  // Paint split (9.2), persisted per session like the brush size. Locks per row.
  let devMix = $state<DevMix>(loadDevMix());
  let devLocks = $state<boolean[]>([false, false, false]);
  $effect(() => saveDevMix(devMix));
  // No-tool hover/selection (9.1b): single-province darken + click-to-edit panel.
  let devHoverId = $state<number>(NONE);
  let devSelectedId = $state<number | null>(null);
  // Effective pending dev components per touched province = base + queue. Drives
  // gradient recolor + the stat overlay before save.
  interface DevEff {
    vals: [number, number, number];
    present: [boolean, boolean, boolean];
  }
  let devEdited = new Map<number, DevEff>();
  // In-progress airbrush accumulators (province -> fractional/committed state).
  let devStroke = new Map<number, DevAccum>();

  // --- Climate mode (Sprint 11.1) -------------------------------------------
  // Base two-slot payload (get_climate); the effective model folds the queue.
  const CLIMATE_FILE = "map/climate.txt";
  let baseClimate = $state<ClimatePayload | null>(null);
  let climateModel = $derived.by<ClimateModel | null>(() => {
    queue.version;
    return baseClimate ? foldClimate(baseClimate, queue.serialize()) : null;
  });
  $effect(() => { if ((mode === "climate" || mode === "winter") && climateModel) untrack(() => openView({kind:"climate", key:mode}, "reuse")); });
  // Per-list province counts for the selector (base + PENDING).
  let climateCountMap = $derived.by<Map<string, number>>(() => {
    const m = climateModel;
    return m ? climateCounts(m) : new Map<string, number>();
  });
  // The selected selector entry: a slot + a list key (null = that slot's eraser).
  let climateSelSlot = $state<ClimateSlot | null>(null);
  let climateSelKey = $state<string | null>(null);
  let hasClimateSel = $derived(climateSelSlot !== null);
  // Winter-tint toggle (climate mode only): overlay winter severity over zones.
  let showWinterTint = $state(false);
  // Live in-stroke override for the painted slot (province id -> key|null).
  let climatePaintOverride = new Map<number, string | null>();
  // Lists created (empty block inserted) during the active stroke, so we don't
  // insert the same `key = { }` twice within one stroke.
  let climateInsertedThisStroke = new Set<string>();
  // Group under the cursor for the ACTIVE slot (hover highlight).
  let climateHoverKey = $state<string | null>(null);
  let climateHovering = $state(false);

  // --- Simple Terrain mode (Sprint 11.2) ------------------------------------
  let baseTerrain = $state<EffectiveTerrainPayload | null>(null);
  let terrainKeyToGroup = new Map<string, number>();
  // province id -> dominant terrain.bmp category (for the "Auto" eraser repaint).
  let terrainAuto = new Map<number, string>();
  // province id -> is-override (base), for the hover status footer.
  let terrainOverrideBase = new Map<number, boolean>();
  // Effective override overlay = base + queue (+ active stroke): province id ->
  // category key, or AUTO_KEY when reverted. Drives repaint + steal eligibility.
  let terrainOverlay = new Map<number, string>();
  let selectedTerrainKey = $state<string | null>(null);
  // Lists (terrain_override blocks) created during the active stroke.
  let terrainInsertedThisStroke = new Set<string>();
  // The province under the cursor / last clicked, for the list's status footer.
  let terrainHover = $state<{ id: number; terrain: string; name: string; isOverride: boolean } | null>(null);
  // Category key -> effective province count (base mode-data + PENDING fold).
  let terrainCounts = $derived.by<Map<string, number>>(() => {
    queue.version;
    const m = new Map<string, number>();
    if (mode !== "simple_terrain" || !modeData || !baseTerrain) return m;
    const overlay = new Map<number, string>();
    for (const e of queue.serialize()) foldTerrainEditInto(overlay, e);
    const vals = modeData.values;
    for (let id = 0; id < vals.length; id++) {
      let key: string | null;
      const ov = overlay.get(id);
      if (ov !== undefined) key = ov === AUTO_KEY ? (terrainAuto.get(id) ?? null) : ov;
      else key = vals[id] !== NONE ? (modeData.groups[vals[id]]?.key ?? null) : null;
      if (key) m.set(key, (m.get(key) ?? 0) + 1);
    }
    return m;
  });
  // Terrain categories with pending modeled-modifier edits folded in (S2.7), so
  // the list summary + property editor seed show base + pending. The commit still
  // diffs against baseTerrain, keeping composites idempotent.
  let effectiveTerrainCategories = $derived.by<TerrainCategory[]>(() => {
    queue.version;
    if (!baseTerrain) return [];
    const edits = queue.serialize();
    return baseTerrain.categories.map((c) => foldTerrainModifiers(c, edits));
  });
  // Re-apply the map recolor when the winter-tint toggle flips (climate mode).
  $effect(() => {
    showWinterTint;
    if ((mode === "climate" || mode === "winter") && !painting && compositor && climateModel) {
      applyClimatePendingToMap();
    }
  });

  // In-progress stroke bookkeeping.
  let painting = false;
  let strokeAffected = new Set<number>();
  let strokeEdits: TypedEdit[] = [];
  let strokeLabel = "";
  let lastPaintX = 0;
  let lastPaintY = 0;
  // Provinces the armed brush would affect at the cursor (live preview).
  let previewSet = new Set<number>();

  const BRUSH_TOOLS = new Set([
    "add_province",
    "remove_province",
    "occupy",
    "restore_control",
    "paint_religion",
    "unpaint_religion",
    "paint_culture",
    "unpaint_culture",
    "tn_add",
    "tn_remove",
    "area_add",
    "area_remove",
    "region_add",
    "region_remove",
    "col_add",
    "col_remove",
    "paint_good",
    "dev_raise",
    "dev_lower",
    "climate_paint",
    "paint_terrain",
    "hre_add",
    "hre_remove",
  ]);
  let brushArmed = $derived(BRUSH_TOOLS.has(armedTool ?? ""));

  let categorical = $derived(modeData?.kind === "categorical");

  // Political mode selection opens the country panel; other categorical modes
  // only show a label + highlight (no panels until later sprints).
  let selectedTag = $derived(
    mode === "political" && categorical && selectedGroup !== NONE
      ? (modeData!.groups[selectedGroup]?.key ?? null)
      : null,
  );
  $effect(() => {
    if (selectedTag) untrack(() => openView({ kind: "country", tag: selectedTag }, "reuse"));
  });

  // Religion mode selection opens the religion panel (Sprint 5.1/5.2).
  let selectedReligionKey = $derived(
    mode === "religion" && categorical && selectedGroup !== NONE
      ? (modeData!.groups[selectedGroup]?.key ?? null)
      : null,
  );

  // Culture mode selection opens the culture panel (Sprint 6.1/6.2).
  let selectedCultureKey = $derived(
    mode === "culture" && categorical && selectedGroup !== NONE
      ? (modeData!.groups[selectedGroup]?.key ?? null)
      : null,
  );

  // Trade Nodes mode selection opens the node panel (Sprint 8.2).
  let selectedNodeKey = $derived(
    mode === "trade_nodes" && categorical && selectedGroup !== NONE
      ? (modeData!.groups[selectedGroup]?.key ?? null)
      : null,
  );
  let selectedNode = $derived(
    selectedNodeKey && tradeNetwork
      ? (tradeNetwork.nodes.find((n) => n.key === selectedNodeKey) ?? null)
      : null,
  );
  let selectedNodeColorPresent = $derived(
    selectedNodeKey != null &&
      (baseColorPresent.has(selectedNodeKey) || createdNodeKeys.has(selectedNodeKey)),
  );

  // Areas / Regions mode selection opens the thin geo panels (Sprint 10).
  let selectedAreaKey = $derived(
    mode === "areas" && categorical && selectedGroup !== NONE
      ? (modeData!.groups[selectedGroup]?.key ?? null)
      : null,
  );
  let selectedArea = $derived<GeoArea | null>(
    selectedAreaKey && geoNetwork
      ? (geoNetwork.areas.find((a) => a.key === selectedAreaKey) ?? null)
      : null,
  );
  let selectedRegionKey = $derived(
    mode === "regions" && categorical && selectedGroup !== NONE
      ? (modeData!.groups[selectedGroup]?.key ?? null)
      : null,
  );
  let selectedRegion = $derived<GeoRegion | null>(
    selectedRegionKey && geoNetwork
      ? (geoNetwork.regions.find((r) => r.key === selectedRegionKey) ?? null)
      : null,
  );

  // Colonial Regions / Trade Companies selection opens the colonial panel (S19).
  let selectedColonialKey = $derived(
    isColonialMode && categorical && selectedGroup !== NONE
      ? (modeData!.groups[selectedGroup]?.key ?? null)
      : null,
  );
  let selectedColonialEntry = $derived<ColonialEntry | null>(
    selectedColonialKey && colonialData
      ? (colonialData.entries.find((e) => e.key === selectedColonialKey) ?? null)
      : null,
  );

  $effect(() => { if (selectedReligionKey) untrack(() => openView({kind:"religion", key:selectedReligionKey}, "reuse")); });
  $effect(() => { if (selectedCultureKey) untrack(() => openView({kind:"culture", key:selectedCultureKey}, "reuse")); });
  $effect(() => { if (selectedNodeKey) untrack(() => openView({kind:"trade-node", key:selectedNodeKey}, "reuse")); });
  $effect(() => { if (selectedAreaKey) untrack(() => openView({kind:"area", key:selectedAreaKey}, "reuse")); });
  $effect(() => { if (selectedRegionKey) untrack(() => openView({kind:"region", key:selectedRegionKey}, "reuse")); });
  $effect(() => { if (selectedColonialKey && isColonialMode) untrack(() => openView({kind:"colonial", colonialKind: mode as "colonial_regions" | "trade_companies", key:selectedColonialKey}, "reuse")); });
  $effect(() => { const index = selectedAdjIndex; if (index != null) untrack(() => openView({kind:"adjacency", index}, "reuse")); });

  // The active paint target for the brush tools: a country tag (political), a
  // religion key, or a culture key. Null when no paintable selection exists.
  let paintTarget = $derived(
    mode === "political"
      ? (armedTool === "hre_add" || armedTool === "hre_remove" ? "hre" : selectedTag)
      : mode === "religion"
        ? selectedReligionKey
        : mode === "culture"
          ? selectedCultureKey
          : mode === "trade_nodes"
            ? selectedNodeKey
            : mode === "areas"
              ? selectedAreaKey
              : mode === "regions"
                ? selectedRegionKey
                : isColonialMode
                ? selectedColonialKey
                : mode === "trade_goods"
                  ? selectedGoodKey
                  : mode === "climate" || mode === "winter"
                    ? (hasClimateSel ? "climate" : null)
                    : mode === "simple_terrain"
                      ? selectedTerrainKey
                      : mode === "development"
                        ? armedTool === "dev_raise" || armedTool === "dev_lower"
                          ? "dev"
                          : null
                        : null,
  );

  // --- Wars / occupation brushes (Sprint 13.3) -----------------------------
  // The selected country's wars at the selected date (backend `get_wars`). Drives
  // the Occupy / Restore-control brushes' enable state + eligibility sets.
  let warsForTag = $state<War[]>([]);
  $effect(() => {
    const cur = selectedTag;
    const d = selectedDate;
    if (mode !== "political" || !cur) {
      warsForTag = [];
      return;
    }
    invoke<War[]>("get_wars", { installPath, modPath, date: d, tag: cur })
      .then((r) => {
        if (selectedTag === cur) warsForTag = r;
      })
      .catch(() => (warsForTag = []));
  });
  // The date the war sets are evaluated at (matches the get_wars fetch).
  let warAt = $derived(selectedDate ?? effectiveStart);
  // ≥1 active war → the Occupy / Restore brushes are offered.
  let hasActiveWarNow = $derived(hasActiveWar(warsForTag, selectedTag, warAt));
  // Occupy paints onto land owned by these (active enemies of the selected tag).
  let occupyEnemySet = $derived(enemyTags(warsForTag, selectedTag, warAt));
  // Restore resets control on land owned by either side of the selected tag's wars.
  let occupyRestoreSet = $derived(belligerentTags(warsForTag, selectedTag, warAt));

  // --- Create country flow (Sprint 4.1, via createEntityFlow) ---
  const countryFlow = createEntityFlow<number, { provinceId: number; name: string }>({
    tool: "create_country",
    defaultName: () => "New Country",
    buildArgs: (provinceId, name) => ({ provinceId, name }),
  });
  let countryFlowState = $state<EntityFlowState<number, { provinceId: number; name: string }>>(
    countryFlow.state,
  );
  $effect(() => countryFlow.subscribe((s) => (countryFlowState = s)));

  // --- Create religion flow (Sprint 5.4, via createEntityFlow) ---
  const religionFlow = createEntityFlow<number, { provinceId: number; name: string }>({
    tool: "create_religion",
    defaultName: () => "New Religion",
    buildArgs: (provinceId, name) => ({ provinceId, name }),
  });
  let religionFlowState = $state<EntityFlowState<number, { provinceId: number; name: string }>>(
    religionFlow.state,
  );
  $effect(() => religionFlow.subscribe((s) => (religionFlowState = s)));

  // --- Create culture flow (Sprint 6.4, via createEntityFlow) ---
  const cultureFlow = createEntityFlow<number, { provinceId: number; name: string }>({
    tool: "create_culture",
    defaultName: () => "New Culture",
    buildArgs: (provinceId, name) => ({ provinceId, name }),
  });
  let cultureFlowState = $state<EntityFlowState<number, { provinceId: number; name: string }>>(
    cultureFlow.state,
  );
  $effect(() => cultureFlow.subscribe((s) => (cultureFlowState = s)));

  // --- Create trade node flow (Sprint 8.5, via createEntityFlow) ---
  const tradeFlow = createEntityFlow<number, { provinceId: number; name: string }>({
    tool: "tn_create",
    defaultName: () => "New Trade Node",
    buildArgs: (provinceId, name) => ({ provinceId, name }),
  });
  let tradeFlowState = $state<EntityFlowState<number, { provinceId: number; name: string }>>(
    tradeFlow.state,
  );
  $effect(() => tradeFlow.subscribe((s) => (tradeFlowState = s)));

  // --- Create area / region flows (Sprint 10, via createEntityFlow) ---
  const areaFlow = createEntityFlow<number, { provinceId: number; name: string }>({
    tool: "area_create",
    defaultName: () => "New Area",
    buildArgs: (provinceId, name) => ({ provinceId, name }),
  });
  let areaFlowState = $state<EntityFlowState<number, { provinceId: number; name: string }>>(
    areaFlow.state,
  );
  $effect(() => areaFlow.subscribe((s) => (areaFlowState = s)));

  const regionFlow = createEntityFlow<number, { provinceId: number; name: string }>({
    tool: "region_create",
    defaultName: () => "New Region",
    buildArgs: (provinceId, name) => ({ provinceId, name }),
  });
  let regionFlowState = $state<EntityFlowState<number, { provinceId: number; name: string }>>(
    regionFlow.state,
  );
  $effect(() => regionFlow.subscribe((s) => (regionFlowState = s)));

  // Colonial region / trade company create flow (Sprint 19).
  const colonialFlow = createEntityFlow<number, { provinceId: number; name: string }>({
    tool: "col_create",
    defaultName: () => "New Colonial Region",
    buildArgs: (provinceId, name) => ({ provinceId, name }),
  });
  let colonialFlowState = $state<EntityFlowState<number, { provinceId: number; name: string }>>(
    colonialFlow.state,
  );
  $effect(() => colonialFlow.subscribe((s) => (colonialFlowState = s)));

  // Provinces mode selection opens the province panel (Sprint 2.1). Each group's
  // key is the province id string; water/wasteland are their own groups too, so
  // they select and show a (read-only) panel.
  let selectedProvinceId = $derived.by(() => {
    if (mode !== "provinces" || !categorical || selectedGroup === NONE || !modeData) return null;
    const k = modeData.groups[selectedGroup]?.key;
    const n = k != null ? parseInt(k, 10) : NaN;
    return Number.isFinite(n) ? n : null;
  });
  $effect(() => {
    if (selectedProvinceId != null) untrack(() => openView({ kind: "province", id: selectedProvinceId }, "reuse"));
  });

  // A tag to auto-select once political mode finishes loading (owner
  // click-through from the province panel).
  let pendingSelectTag: string | null = null;
  // An area key to auto-select once Areas mode finishes loading (region-panel
  // member-area jump).
  let pendingSelectGeoKey: string | null = null;

  /// Switch to political mode and select `tag` (province-panel owner link).
  function openCountryInPolitical(tag: string, tab?: "overview" | "rulers" | "ideas" | "diplomacy" | "estates" | "history" | "names") {
    openView({ kind: "country", tag, ...(tab ? { tab } : {}) }, "reuse");
  }

  // A culture key to auto-select once Culture mode finishes loading (province
  // panel reverse-names jump, Sprint 24).
  let pendingSelectCultureKey: string | null = null;
  /// Switch to culture mode and select `key` (reverse province-names jump).
  function openCultureMode(key: string) {
    openView({ kind: "culture", key }, "reuse");
  }

  // The localized label to show bottom-left: hovered group wins, else selected.
  let labelName = $derived.by(() => {
    if (!categorical || !modeData) return null;
    const g = hoverGroup !== NONE ? hoverGroup : selectedGroup;
    return g !== NONE ? (modeData.groups[g]?.label ?? null) : null;
  });

  // Occupation footer (Sprint 13.3): when hovering a striped (occupied) province
  // in political mode, its controller's name. Hit-testing stays owner-based.
  let occupiedByLabel = $derived.by<string | null>(() => {
    queue.version; // recompute after a pending occupy/restore edit
    if (mode !== "political" || politicalHoverId === NONE || !modeData) return null;
    const s = effOf(politicalHoverId);
    if (s.owner == null || s.controller == null || s.controller === s.owner) return null;
    if (s.controller === "REB") return "Rebels";
    const gi = tagToGroup.get(s.controller);
    return gi !== undefined ? (modeData.groups[gi]?.label ?? s.controller) : s.controller;
  });

  /// Loads the per-pixel province id buffer once; cached for the session.
  async function ensureProvinceIds() {
    if (provinceIds) return;
    const idsBuf = await invoke<ArrayBuffer>("get_province_ids", {
      installPath,
      modPath,
    });
    const header = new Uint32Array(idsBuf, 0, 2);
    mapW = header[0];
    mapH = header[1];
    provinceIds = new Uint16Array(idsBuf, 8, mapW * mapH);
  }

  /// Computes province centroids once per id-buffer (shared by the trade-node
  /// overlay + marker hit-testing); cached by buffer identity.
  function ensureCentroids() {
    if (!provinceIds) return;
    if (centroidsSource === provinceIds) return;
    centroids = computeCentroids(provinceIds, mapW, mapH);
    centroidsSource = provinceIds;
  }

  /// Runs the areas validation domain (covers area + region + superregion
  /// orphans) for the persistent panel strip in Areas/Regions modes.
  async function runGeoValidation() {
    try {
      geoIssues = await invoke<ValidationIssue[]>("validate", {
        domain: "areas",
        installPath,
        modPath,
        date: selectedDate,
      });
    } catch {
      geoIssues = [];
    }
  }

  /// Builds the offscreen compositing pipeline from the freshly rendered PNG.
  function buildCompositor() {
    if (!bitmap || !provinceIds) return;
    const off = document.createElement("canvas");
    off.width = bitmap.width;
    off.height = bitmap.height;
    const octx = off.getContext("2d")!;
    octx.drawImage(bitmap, 0, 0);
    const pristine = octx.getImageData(0, 0, off.width, off.height);
    compositor = new MapCompositor(pristine, provinceIds);
  }

  // --- Province Colors edit layer ------------------------------------------

  /// Every pending province-bitmap op in the queue, in order (static edits, so
  /// the full serialize — not the date-gated one).
  function pendingBmpOps(): BmpOp[] {
    const ops: BmpOp[] = [];
    for (const e of queue.serialize()) {
      if (e.kind === "provinceBmp" && e.file === "map/provinces.bmp") ops.push(...e.ops);
    }
    return ops;
  }

  /// Snapshots the freshly rendered province_colors bitmap as the pristine
  /// (saved) image, then applies pending ops so the canvas shows current state.
  function buildPcCanvas() {
    if (!bitmap) return;
    const off = pcEditCanvas ?? document.createElement("canvas");
    off.width = bitmap.width;
    off.height = bitmap.height;
    const octx = off.getContext("2d")!;
    octx.drawImage(bitmap, 0, 0);
    pcPristine = octx.getImageData(0, 0, off.width, off.height);
    pcEditCanvas = off;
    pcOverlay = pcOverlay ?? document.createElement("canvas");
    pcOverlay.width = off.width;
    pcOverlay.height = off.height;
    // Build the color → province-id lookup from the saved bitmap (every pixel is
    // a definition color, so this is exact), plus any provinces already carved
    // this session (their color isn't in the just-rendered pristine yet).
    if (provinceIds) {
      const lut = new Uint16Array(1 << 24).fill(NONE);
      const pd = pcPristine.data;
      for (let i = 0; i < provinceIds.length; i++) {
        lut[(pd[i * 4] << 16) | (pd[i * 4 + 1] << 8) | pd[i * 4 + 2]] = provinceIds[i];
      }
      for (const c of pcCreated) {
        lut[(c.color[0] << 16) | (c.color[1] << 8) | c.color[2]] = c.id;
      }
      pcColorLut = lut;
    }
    pcIds = null;
    pcIdsDirty = true;
    rebuildPcCanvas();
    rebuildPcOverlay();
  }

  /// The edit-aware province-id buffer: `provinceIds` reprojected through the
  /// current (pending-edited) bitmap colors. Falls back to `provinceIds` until
  /// the pc canvas/lut are built. Rebuilt lazily when pixels change.
  function getPcIds(): Uint16Array | null {
    if (!provinceIds) return null;
    if (!pcColorLut || !pcCurImage) return provinceIds;
    if (!pcIds || pcIds.length !== provinceIds.length) {
      pcIds = new Uint16Array(provinceIds.length);
      pcIdsDirty = true;
    }
    if (pcIdsDirty) {
      const d = pcCurImage.data;
      for (let i = 0; i < pcIds.length; i++) {
        pcIds[i] = pcColorLut[(d[i * 4] << 16) | (d[i * 4 + 1] << 8) | d[i * 4 + 2]];
      }
      pcIdsDirty = false;
    }
    return pcIds;
  }

  /// Province id under the cursor, edit-aware (the province you just drew, not
  /// the one underneath).
  function pcProvinceIdAt(clientX: number, clientY: number): number {
    const ids = getPcIds();
    const px = pcPixelAt(clientX, clientY);
    if (!ids || !px) return NONE;
    return ids[px[1] * mapW + px[0]];
  }

  /// Re-derives the displayed bitmap = pristine + every pending op (keeps
  /// undo/redo exact without a backend re-render).
  function rebuildPcCanvas() {
    if (!pcEditCanvas || !pcPristine) return;
    const img = new ImageData(
      new Uint8ClampedArray(pcPristine.data),
      pcPristine.width,
      pcPristine.height,
    );
    applyOpsToRgba(img.data, pendingBmpOps(), mapW, mapH);
    pcEditCanvas.getContext("2d")!.putImageData(img, 0, 0);
    pcCurImage = img;
    pcIdsDirty = true;
  }

  /// Rebuilds the source/target tint overlay (Dissolve source = warm, targets =
  /// cool). Only recomputed on selection change, not per redraw.
  function rebuildPcOverlay() {
    const ids = getPcIds();
    if (!pcOverlay || !ids) return;
    const octx = pcOverlay.getContext("2d")!;
    const img = octx.createImageData(pcOverlay.width, pcOverlay.height);
    const d = img.data;
    if (pcSelectedId != null || pcTargets.size > 0) {
      for (let i = 0; i < ids.length; i++) {
        const id = ids[i];
        const o = i * 4;
        if (id === pcSelectedId) {
          d[o] = 255; d[o + 1] = 120; d[o + 2] = 60; d[o + 3] = 120;
        } else if (pcTargets.has(id)) {
          d[o] = 80; d[o + 1] = 200; d[o + 2] = 120; d[o + 3] = 120;
        }
      }
    }
    octx.putImageData(img, 0, 0);
  }

  /// Current bitmap color of a province, sampled from the displayed image via
  /// the edit-aware id buffer (so a just-carved province samples ITS color).
  function pcColorOf(id: number): Rgb | null {
    const ids = getPcIds();
    if (!ids || !pcCurImage) return null;
    return provinceColor(ids, pcCurImage.data, id);
  }

  /// Map pixel (x,y) under the cursor, or null off-map.
  function pcPixelAt(clientX: number, clientY: number): [number, number] | null {
    const [mx, my] = toMap(clientX, clientY);
    const x = Math.floor(mx);
    const y = Math.floor(my);
    if (x < 0 || y < 0 || x >= mapW || y >= mapH) return null;
    return [x, y];
  }

  /// Stamps `pixels` on the displayed canvas with `color` (efficient dirty-rect
  /// putImageData) and repaints.
  function pcStamp(pixels: number[], color: Rgb) {
    if (!pcEditCanvas || !pcCurImage || pixels.length === 0) return;
    const d = pcCurImage.data;
    let minX = mapW, minY = mapH, maxX = 0, maxY = 0;
    for (const p of pixels) {
      const o = p * 4;
      d[o] = color[0]; d[o + 1] = color[1]; d[o + 2] = color[2];
      const x = p % mapW;
      const y = (p / mapW) | 0;
      if (x < minX) minX = x;
      if (x > maxX) maxX = x;
      if (y < minY) minY = y;
      if (y > maxY) maxY = y;
    }
    if (maxX < minX) return;
    pcEditCanvas
      .getContext("2d")!
      .putImageData(pcCurImage, 0, 0, minX, minY, maxX - minX + 1, maxY - minY + 1);
    pcIdsDirty = true;
    redraw();
  }

  function clearPcSelection() {
    pcSelectedId = null;
    pcTargets = new Set();
    rebuildPcOverlay();
    redraw();
  }

  // Rebuild the displayed bitmap whenever the queue changes (undo/redo/save) so
  // the province_colors view always reflects pending ops.
  $effect(() => {
    queue.version;
    if (mode === "province_colors" && pcEditCanvas) {
      untrack(() => {
        rebuildPcCanvas();
        redraw();
      });
    }
  });

  // Pointer lifecycle for the province-colors tools (routed from the shared
  // onPointerDown/Move/Up). New/Expand paint on drag; a non-drag press is a
  // click (select / absorb / toggle a dissolve target).
  function pcOnDown(clientX: number, clientY: number) {
    pcPointerActive = true;
    pcPainting = false;
    pcMoved = 0;
    pcDownX = clientX;
    pcDownY = clientY;
    pcStrokePixels = new Set();
    pcSourceId = pcProvinceIdAt(clientX, clientY);
    if (armedTool === "pc_new") {
      pcStrokeColor = [255, 0, 255]; // placeholder until the backend allocates the real color
    } else if (armedTool === "pc_expand" && pcSelectedId != null) {
      pcStrokeColor = pcColorOf(pcSelectedId);
    } else {
      pcStrokeColor = null;
    }
  }

  function pcOnMove(clientX: number, clientY: number) {
    pcMoved = Math.max(pcMoved, Math.abs(clientX - pcDownX) + Math.abs(clientY - pcDownY));
    setBrushCursor(clientX, clientY, true);
    const canPaint =
      armedTool === "pc_new" || (armedTool === "pc_expand" && pcSelectedId != null);
    if (!canPaint || !pcStrokeColor) return;
    if (pcMoved > 4) pcPainting = true;
    if (!pcPainting) return;
    const px = pcPixelAt(clientX, clientY);
    if (!px) return;
    const stamp = brushDisc(px[0], px[1], brushSize, mapW, mapH);
    for (const p of stamp) pcStrokePixels.add(p);
    pcStamp(stamp, pcStrokeColor);
  }

  function pcOnUp(clientX: number, clientY: number, shift: boolean) {
    pcPointerActive = false;
    const wasPaint = pcPainting;
    pcPainting = false;
    if (wasPaint) {
      if (armedTool === "pc_new") openPcNamePrompt(clientX, clientY);
      else if (armedTool === "pc_expand") commitExpandStroke();
      return;
    }
    // A click (no drag).
    const id = pcProvinceIdAt(clientX, clientY);
    if (id === NONE) return;
    if (armedTool === "pc_expand") pcClickExpand(id, shift);
    else if (armedTool === "pc_dissolve") pcClickDissolve(id);
  }

  function commitExpandStroke() {
    if (!pcStrokeColor || pcStrokePixels.size === 0 || pcSelectedId == null) {
      rebuildPcCanvas();
      redraw();
      return;
    }
    const op = paintOp([...pcStrokePixels], pcStrokeColor);
    queue.push({
      label: `Expand province ${pcSelectedId}`,
      edits: [{ kind: "provinceBmp", file: "map/provinces.bmp", ops: [op] }],
    });
  }

  function pcClickExpand(id: number, shift: boolean) {
    if (pcSelectedId == null) {
      pcSelectedId = id;
      rebuildPcOverlay();
      redraw();
      return;
    }
    if (shift) {
      const ids = getPcIds();
      if (id === pcSelectedId || !ids) return;
      const color = pcColorOf(pcSelectedId);
      if (!color) return;
      const pixels = provincePixels(ids, id);
      if (pixels.length === 0) return;
      pcStamp(pixels, color);
      queue.push({
        label: `Absorb province ${id} into ${pcSelectedId}`,
        edits: [{ kind: "provinceBmp", file: "map/provinces.bmp", ops: [paintOp(pixels, color)] }],
      });
    } else {
      pcSelectedId = id; // reselect the province to grow
      rebuildPcOverlay();
      redraw();
    }
  }

  function pcClickDissolve(id: number) {
    const ids = getPcIds();
    if (!ids) return;
    const setSource = () => {
      pcSelectedId = id;
      pcDissolveCandidates = new Set(borderingProvinces(ids, id, mapW, mapH));
      pcTargets = new Set();
      rebuildPcOverlay();
      redraw();
    };
    if (pcSelectedId == null) {
      setSource();
    } else if (id === pcSelectedId) {
      clearPcSelection();
      pcDissolveCandidates = new Set();
    } else if (pcDissolveCandidates.has(id)) {
      const next = new Set(pcTargets);
      next.has(id) ? next.delete(id) : next.add(id);
      pcTargets = next;
      rebuildPcOverlay();
      redraw();
    } else {
      setSource(); // clicked a non-neighbour → start over on it
    }
  }

  function confirmDissolve() {
    if (pcSelectedId == null || pcTargets.size === 0) return;
    const from = pcColorOf(pcSelectedId);
    if (!from) return;
    const into: Rgb[] = [];
    for (const t of pcTargets) {
      const c = pcColorOf(t);
      if (c) into.push(c);
    }
    if (into.length === 0) return;
    queue.push({
      label: `Dissolve province ${pcSelectedId} into ${into.length}`,
      edits: [{ kind: "provinceBmp", file: "map/provinces.bmp", ops: [dissolveOp(from, into)] }],
    });
    clearPcSelection();
    pcDissolveCandidates = new Set();
  }

  function openPcNamePrompt(clientX: number, clientY: number) {
    if (pcStrokePixels.size === 0) return;
    if (pcSourceId === NONE) {
      // Carved over water/off-map: no area to inherit — revert the placeholder.
      rebuildPcCanvas();
      redraw();
      error = "Carve a new province starting over an existing land province.";
      return;
    }
    pcNamePrompt = { x: clientX, y: clientY, pixels: [...pcStrokePixels], sourceId: pcSourceId };
    pcNewName = "New Province";
  }

  interface ProvinceScaffoldResult {
    id: number;
    color: Rgb;
    name: string;
    area: string;
    edits: TypedEdit[];
  }

  async function acceptPcName(name: string) {
    const prompt = pcNamePrompt;
    pcNamePrompt = null;
    if (!prompt) return;
    try {
      const scaffold = await invoke<ProvinceScaffoldResult>("add_province_scaffold", {
        installPath,
        modPath,
        pixels: prompt.pixels,
        name,
        sourceId: prompt.sourceId,
      });
      queue.push({ label: `Add province ${name}`, edits: scaffold.edits });
      // Register the new province's color→id so it becomes independently
      // selectable (dissolve/expand) immediately, before any save.
      pcCreated.push({ color: scaffold.color, id: scaffold.id });
      if (pcColorLut) {
        pcColorLut[
          (scaffold.color[0] << 16) | (scaffold.color[1] << 8) | scaffold.color[2]
        ] = scaffold.id;
      }
      pcIdsDirty = true;
      // The queue effect rebuilds the canvas with the real allocated color.
    } catch (e) {
      error = String(e);
      rebuildPcCanvas(); // drop the placeholder paint
      redraw();
    }
  }

  function cancelPcName() {
    pcNamePrompt = null;
    rebuildPcCanvas();
    redraw();
  }

  /// Loads interaction data for `m` and (for categorical modes) builds the
  /// compositor. `seq` guards against a stale mode landing after a newer one.
  async function loadModeInteraction(m: string, seq: number) {
    compositor = null;
    modeData = null;
    hoverGroup = NONE;
    hovering = false;
    selectedGroup = NONE;
    previewSet = new Set();
    pendingGroup = new Map();
    tagToGroup = new Map();
    religionKeyToGroup = new Map();
    religionEdited = new Map();
    cultureKeyToGroup = new Map();
    cultureEdited = new Map();
    cultureColorOverrides = new Map();
    baseNetwork = null;
    tradeNodeKeyToGroup = new Map();
    tnMemberCache = new Map();
    tnPaintOverride = new Map();
    selectedRoute = null;
    hoverNode = null;
    hoverRoute = null;
    editControl = null;
    draggingHandle = -1;
    baseGeo = null;
    areaKeyToGroup = new Map();
    regionKeyToGroup = new Map();
    geoMemberCache = new Map();
    geoPaintOverride = new Map();
    geoIssues = [];
    tradeGoodKeyToGroup = new Map();
    tradeGoodEdited = new Map();
    pendingTradeGoodColors = new Map();
    baseGoods = [];
    createdGoods = [];
    selectedGoodKey = null;
    if (overlayAtlas?.image && "close" in overlayAtlas.image) {
      (overlayAtlas.image as ImageBitmap).close();
    }
    overlayAtlas = null;
    overlayAtlasIndex = new Map();
    overlayItems = new Map();
    tradeDetailBase = new Map();
    cotEdited = new Map();
    tradeDetailTooltip = null;
    devEdited = new Map();
    devStroke = new Map();
    devHoverId = NONE;
    devSelectedId = null;
    baseClimate = null;
    climateSelSlot = null;
    climateSelKey = null;
    climatePaintOverride = new Map();
    climateInsertedThisStroke = new Set();
    climateHoverKey = null;
    climateHovering = false;
    baseTerrain = null;
    terrainKeyToGroup = new Map();
    terrainAuto = new Map();
    terrainOverrideBase = new Map();
    terrainOverlay = new Map();
    terrainInsertedThisStroke = new Set();
    selectedTerrainKey = null;
    terrainHover = null;

    const dataBuf = await invoke<ArrayBuffer>("get_mode_data", {
      installPath,
      modPath,
      mode: m,
      date: selectedDate,
    });
    if (seq !== renderSeq) return;
    const md = parseModeData(dataBuf);
    // Development is a gradient, but it still uses the compositor for client-side
    // pending recolor (mirrors the dev ramp so paints show without a re-render).
    if (md.kind === "categorical" || m === "development") {
      await ensureProvinceIds();
      if (seq !== renderSeq) return;
      buildCompositor();
    }
    // Province Colors: build the editable pixel canvas (needs the id buffer for
    // whole-province ops, dissolve targets, and color sampling).
    if (m === "province_colors") {
      await ensureProvinceIds();
      if (seq !== renderSeq) return;
      buildPcCanvas();
    }
    if (m === "development") {
      // Bulk data carries the three dev components per province (paint + stats);
      // the atlas supplies the stat-box icons.
      await ensureProvincePolitical();
      if (seq !== renderSeq) return;
      await loadOverlayAtlas(m, seq);
      if (seq !== renderSeq) return;
    }
    if (m === "political") {
      md.groups.forEach((g, i) => tagToGroup.set(g.key, i));
      // Bulk political data backs paint eligibility + insert-vs-replace; load it
      // now so pending owners repaint even before a brush is armed.
      await ensureProvincePolitical();
      if (seq !== renderSeq) return;
    }
    if (m === "religion") {
      religionKeyToGroup = new Map();
      md.groups.forEach((g, i) => religionKeyToGroup.set(g.key, i));
      // Province files back paint eligibility (land/water) + file resolution.
      await ensureProvincePolitical();
      if (seq !== renderSeq) return;
    }
    if (m === "culture") {
      cultureKeyToGroup = new Map();
      md.groups.forEach((g, i) => cultureKeyToGroup.set(g.key, i));
      await ensureProvincePolitical();
      if (seq !== renderSeq) return;
      // Pre-load display-color overrides so they paint immediately (the rendered
      // PNG has hash colors baked in; overrides recolor on top).
      try {
        const overrides = await invoke<Record<string, Rgb>>("list_culture_color_overrides", {
          modPath,
        });
        cultureColorOverrides = new Map(Object.entries(overrides));
      } catch {
        cultureColorOverrides = new Map();
      }
      if (seq !== renderSeq) return;
    }
    if (m === "provinces") {
      // Adjacency overlay: centroids place the lines; get_adjacencies backs them.
      ensureCentroids();
      await loadAdjacencies(seq);
      if (seq !== renderSeq) return;
    }
    if (m === "trade_nodes") {
      tradeNodeKeyToGroup = new Map();
      md.groups.forEach((g, i) => tradeNodeKeyToGroup.set(g.key, i));
      // Province files back the unassigned tint; centroids place markers.
      await ensureProvincePolitical();
      if (seq !== renderSeq) return;
      const net = await invoke<TradeNetwork>("get_trade_network", { installPath, modPath });
      if (seq !== renderSeq) return;
      baseNetwork = net;
      baseColorPresent = new Set(net.nodes.filter((n) => n.color != null).map((n) => n.key));
      createdNodeKeys = new Set();
      ensureCentroids();
      // Setting `baseNetwork` above re-derives `tradeNetwork` → `tradeIssues`
      // (client-side S2.8 checks); no explicit validation run needed.
      // Trade-detail overlay (S3.3): CoT icons + trade-modifier badges.
      await loadOverlayAtlas(m, seq);
      if (seq !== renderSeq) return;
      await loadTradeDetails(seq);
      if (seq !== renderSeq) return;
    }
    if (m === "areas" || m === "regions") {
      const keyToGroup = m === "areas" ? new Map<string, number>() : new Map<string, number>();
      md.groups.forEach((g, i) => keyToGroup.set(g.key, i));
      if (m === "areas") areaKeyToGroup = keyToGroup;
      else regionKeyToGroup = keyToGroup;
      // Province files back paint eligibility (land only) + file resolution.
      await ensureProvincePolitical();
      if (seq !== renderSeq) return;
      const net = await invoke<GeoNetwork>("get_geo_network", { installPath, modPath });
      if (seq !== renderSeq) return;
      baseGeo = net;
      runGeoValidation();
    }
    if (m === "colonial_regions" || m === "trade_companies") {
      colonialKeyToGroup = new Map();
      md.groups.forEach((g, i) => colonialKeyToGroup.set(g.key, i));
      // Province files back paint eligibility (land only) + file resolution.
      await ensureProvincePolitical();
      if (seq !== renderSeq) return;
      const data = await invoke<ColonialData>("get_colonial_data", { kind: m, installPath, modPath });
      if (seq !== renderSeq) return;
      baseColonial = data;
      runColonialValidation();
    }
    if (m === "trade_goods") {
      tradeGoodKeyToGroup = new Map();
      // Cluster groups (`unknown#N`) also register their BASE key first-wins,
      // so paint/recolor lookups by good key resolve to a representative group
      // (all clusters share the unknown good's color, so any works).
      md.groups.forEach((g, i) => {
        tradeGoodKeyToGroup.set(g.key, i);
        const bk = goodKeyOfGroup(g.key);
        if (!tradeGoodKeyToGroup.has(bk)) tradeGoodKeyToGroup.set(bk, i);
      });
      // Province files back paint eligibility (land only) + file resolution.
      await ensureProvincePolitical();
      if (seq !== renderSeq) return;
      // The full goods list (base + mod) for the right-side list.
      try {
        const payload = await invoke<TradeGoodsPayload>("get_trade_goods", {
          installPath,
          modPath,
        });
        if (seq !== renderSeq) return;
        baseGoods = payload.goods;
      } catch {
        baseGoods = [];
      }
      // The icon atlas (7.6) — also feeds the list-row icons.
      await loadOverlayAtlas(m, seq);
      if (seq !== renderSeq) return;
    }
    if (m === "climate" || m === "winter") {
      // Land/water eligibility + file resolution; the two-slot payload backs the
      // selector, recolor, and steal-within-slot painting (Sprint 11.1).
      await ensureProvincePolitical();
      if (seq !== renderSeq) return;
      try {
        baseClimate = await invoke<ClimatePayload>("get_climate", { installPath, modPath });
      } catch {
        baseClimate = null;
      }
      if (seq !== renderSeq) return;
    }
    if (m === "simple_terrain") {
      // Categorical (groups = terrain categories); mirror trade_goods list-driven
      // painting but on terrain_override lists (Sprint 11.2).
      terrainKeyToGroup = new Map();
      md.groups.forEach((g, i) => terrainKeyToGroup.set(g.key, i));
      await ensureProvincePolitical();
      if (seq !== renderSeq) return;
      try {
        const payload = await invoke<EffectiveTerrainPayload>("get_effective_terrain", {
          installPath,
          modPath,
        });
        baseTerrain = payload;
        terrainAuto = new Map();
        terrainOverrideBase = new Map();
        for (const p of payload.provinces) {
          if (p.autoTerrain) terrainAuto.set(p.id, p.autoTerrain);
          terrainOverrideBase.set(p.id, p.isOverride);
        }
        // Mod-added categories with no mode-data group still need one for painting.
        for (const c of payload.categories) {
          if (!terrainKeyToGroup.has(c.key)) {
            const gi = md.groups.length;
            md.groups.push({ key: c.key, label: c.name, color: c.color });
            terrainKeyToGroup.set(c.key, gi);
          }
        }
      } catch {
        baseTerrain = null;
      }
      if (seq !== renderSeq) return;
    }
    modeData = md;
    // Re-apply any queued political edits onto the freshly built compositor.
    refreshPending();
    // Owner click-through: select the requested tag now that political data is in.
    if (m === "political" && pendingSelectTag) {
      const gi = tagToGroup.get(pendingSelectTag);
      pendingSelectTag = null;
      if (gi !== undefined) select(gi);
    }
    // Province jump (from the religion panel usage links).
    if (m === "provinces" && pendingSelectProvince != null) {
      const want = String(pendingSelectProvince);
      pendingSelectProvince = null;
      const gi = md.groups.findIndex((g) => g.key === want);
      if (gi >= 0) select(gi);
    }
    // Area jump (from a region panel's member-area link).
    if (m === "areas" && pendingSelectGeoKey != null) {
      const gi = areaKeyToGroup.get(pendingSelectGeoKey);
      pendingSelectGeoKey = null;
      if (gi !== undefined) select(gi);
    }
    // Culture jump (from the province panel's reverse province-names view).
    if (m === "culture" && pendingSelectCultureKey != null) {
      const gi = cultureKeyToGroup.get(pendingSelectCultureKey);
      pendingSelectCultureKey = null;
      if (gi !== undefined) select(gi);
    }
    // Trade-node jump (from the Problems dashboard, Sprint 30.2).
    if (m === "trade_nodes" && pendingSelectNodeKey != null) {
      const gi = tradeNodeKeyToGroup.get(pendingSelectNodeKey);
      pendingSelectNodeKey = null;
      if (gi !== undefined) {
        selectedRoute = null;
        select(gi);
      }
    }
    // Colonial-region / trade-company jump (from the Problems dashboard).
    if ((m === "colonial_regions" || m === "trade_companies") && pendingSelectColonialKey != null) {
      const gi = colonialKeyToGroup.get(pendingSelectColonialKey);
      pendingSelectColonialKey = null;
      if (gi !== undefined) select(gi);
    }
  }

  /// Group index under a screen point, or NONE. Respects the NONE sentinel:
  /// a province outside the mode (ocean in religion, unowned land, …) yields
  /// NONE, so it can never match a NONE hover/selection.
  function groupAt(clientX: number, clientY: number): number {
    if (!provinceIds || !modeData || modeData.kind !== "categorical") return NONE;
    const rect = canvas.getBoundingClientRect();
    const x = Math.floor((clientX - rect.left - offsetX) / scale);
    const y = Math.floor((clientY - rect.top - offsetY) / scale);
    if (x < 0 || y < 0 || x >= mapW || y >= mapH) return NONE;
    const id = provinceIds[y * mapW + x];
    return effectiveGroup(id);
  }

  /// Pending-aware group index for a province id: the pending owner's group if
  /// this province was repainted by a queued/stroke edit, else the base mode
  /// value. Both hit-testing and highlight read this so hovering a just-painted
  /// province reflects its new country (base + mod + PENDING).
  function effectiveGroup(id: number): number {
    if (!modeData) return NONE;
    const p = pendingGroup.get(id);
    if (p !== undefined) return p;
    return id < modeData.values.length ? modeData.values[id] : NONE;
  }

  /// Map coordinates of a screen point (used for brush painting/preview).
  function toMap(clientX: number, clientY: number): [number, number] {
    const rect = canvas.getBoundingClientRect();
    return [(clientX - rect.left - offsetX) / scale, (clientY - rect.top - offsetY) / scale];
  }

  /// Rebuilds the darken overlay for the hovered/selected group through the
  /// compositor — the single highlight code path (no per-mode duplication).
  function updateHighlight() {
    if (!compositor || !modeData) return;
    if (mode === "development") {
      updateDevHighlight();
      return;
    }
    if (mode === "climate" || mode === "winter") {
      updateClimateHighlight();
      return;
    }
    provinceFill.fill(0);
    const tradeExtra =
      mode === "trade_nodes" && (showUnassigned || hoverRoute !== null || selectedRoute !== null);
    const hasHighlight =
      hoverGroup !== NONE || selectedGroup !== NONE || previewSet.size > 0 || tradeExtra;
    if (hasHighlight) {
      const vals = modeData.values;
      for (let id = 0; id < vals.length; id++) {
        // Brush preview wins over hover/selection — it's the actionable feedback.
        if (previewSet.has(id)) {
          provinceFill[id] = PREVIEW_FILL;
          continue;
        }
        const g = effectiveGroup(id);
        // List-selected "unknown" = ALL undiscovered: every cluster group plus
        // absent-good land. Distinguished from a map click on one cluster by
        // selectedGroup being NONE (selectGood leaves it NONE for UNKNOWN_KEY).
        const allUndiscovered =
          mode === "trade_goods" && selectedGoodKey === UNKNOWN_KEY && selectedGroup === NONE;
        if (g === NONE) {
          // Show-unassigned tint: provinces belonging to no node (Sprint 8.1).
          if (mode === "trade_nodes" && showUnassigned) provinceFill[id] = UNASSIGNED_FILL;
          // "No trade good" also covers land provinces lacking a good key (absent).
          else if (allUndiscovered) {
            const b = baseProv?.get(id);
            if (b && !b.water && !b.wasteland) provinceFill[id] = SELECTED_FILL;
          }
          continue;
        }
        if (g === selectedGroup) provinceFill[id] = SELECTED_FILL;
        else if (
          allUndiscovered &&
          goodKeyOfGroup(modeData.groups[g]?.key ?? "") === UNKNOWN_KEY
        )
          provinceFill[id] = SELECTED_FILL;
        else if (g === hoverGroup) provinceFill[id] = HOVER_FILL;
      }
      // Emphasize the hovered/selected route's path provinces (over the above).
      if (mode === "trade_nodes" && tradeNetwork) {
        const rr = hoverRoute ?? selectedRoute;
        if (rr) {
          const n = tradeNetwork.nodes.find((x) => x.key === rr.from);
          const route = n?.outgoing[rr.index];
          if (route) for (const pid of route.path) if (pid < provinceFill.length) provinceFill[pid] = ROUTE_FILL;
        }
      }
    }
    // HRE member highlight (Sprint 29): tint Empire provinces in political mode,
    // under any hover/selection/preview already painted above.
    if (mode === "political" && hreHighlightIds && hreHighlightIds.size > 0) {
      for (const id of hreHighlightIds) {
        if (id < provinceFill.length && provinceFill[id] === 0) provinceFill[id] = HRE_FILL;
      }
    }
    compositor.setHighlight(provinceFill);
  }

  /// Development highlight (9.1b): single-province hover/selection darken, plus
  /// the brush preview when a dev tool is armed. No group model (gradient mode).
  function updateDevHighlight() {
    if (!compositor) return;
    provinceFill.fill(0);
    if (previewSet.size > 0) {
      for (const id of previewSet) if (id < provinceFill.length) provinceFill[id] = PREVIEW_FILL;
    }
    if (devSelectedId != null && devSelectedId < provinceFill.length && !previewSet.has(devSelectedId)) {
      provinceFill[devSelectedId] = SELECTED_FILL;
    }
    if (
      devHoverId !== NONE &&
      devHoverId < provinceFill.length &&
      devHoverId !== devSelectedId &&
      !previewSet.has(devHoverId)
    ) {
      provinceFill[devHoverId] = HOVER_FILL;
    }
    compositor.setHighlight(provinceFill);
  }

  /// Development single-province hover (no tool armed): darken the land province
  /// under the cursor, mirroring Provinces mode's per-province feedback.
  function devHover(clientX: number, clientY: number) {
    const id = provinceIdAt(clientX, clientY);
    const b = id !== NONE ? baseProv?.get(id) : undefined;
    const h = b && !b.water && !b.wasteland ? id : NONE;
    if (h === devHoverId) return;
    devHoverId = h;
    hovering = h !== NONE;
    updateHighlight();
    redraw();
  }

  function setHover(g: number) {
    if (g === hoverGroup) return;
    hoverGroup = g;
    hovering = g !== NONE;
    updateHighlight();
    redraw();
  }

  function select(g: number) {
    if (g === selectedGroup) return;
    selectedGroup = g;
    updateHighlight();
    redraw();
  }

  // --- Pending political projection (base + mod + PENDING) -----------------
  //
  // `edited` holds the effective political state of every province touched by a
  // queued edit or the active stroke. It is rebuilt by folding the flattened
  // edit queue over the base bulk data (so undo/redo are honored), and mutated
  // incrementally while a stroke is in progress. From it we derive both the
  // recolor overrides (compositor.setOverrides) and the pendingGroup map that
  // hit-testing/highlight read — the single "pending state is queryable" path.

  /// Loads the bulk per-province political payload once per session.
  async function ensureProvincePolitical() {
    if (baseProv) return;
    const list = await invoke<ProvincePolitical[]>("get_province_political", {
      installPath,
      modPath,
      date: selectedDate,
    });
    const m = new Map<number, ProvincePolitical>();
    for (const p of list) m.set(p.id, p);
    baseProv = m;
  }

  /// Fresh effective state for a province, cloned from its base bulk record.
  function baseEff(id: number): Eff {
    const b = baseProv?.get(id);
    return {
      owner: b?.owner ?? null,
      ownerPresent: b?.owner != null,
      controller: b?.controller ?? null,
      controllerPresent: b?.controller != null,
      cores: new Set(b?.cores ?? []),
    };
  }

  /// Read-only effective state (edited overlay, else base).
  function effOf(id: number): Eff {
    return edited.get(id) ?? baseEff(id);
  }

  /// Mutable effective state for a province, materialized in `edited`.
  function mutEff(id: number): Eff {
    let e = edited.get(id);
    if (!e) {
      e = baseEff(id);
      edited.set(id, e);
    }
    return e;
  }

  /// Parses a province id out of a `history/provinces/<id> - name.txt` path.
  function idFromFile(file: string): number | null {
    const base = file.slice(file.lastIndexOf("/") + 1);
    const m = base.match(/^(\d+)/);
    return m ? parseInt(m[1], 10) : null;
  }

  /// Folds one queued edit into the effective state (province-history edits
  /// only). Uses the shared normalizer so top-level (start-date) and dated-block
  /// (later-date) edits both project onto the political overlay.
  function foldEdit(e: TypedEdit) {
    const m = provinceEditMutations(e);
    if (!m || !m.file.startsWith("history/provinces/")) return;
    const id = idFromFile(m.file);
    if (id === null) return;
    const s = mutEff(id);
    for (const mut of m.muts) {
      if (mut.key === "owner") {
        if (mut.remove) {
          s.owner = null;
          s.ownerPresent = false;
        } else {
          s.owner = mut.value;
          s.ownerPresent = true;
        }
      } else if (mut.key === "controller") {
        s.controllerPresent = !mut.remove;
        s.controller = mut.remove ? null : mut.value;
      } else if (mut.key === "add_core") {
        if (mut.remove) s.cores.delete(mut.value);
        else s.cores.add(mut.value);
      } else if (mut.key === "remove_core") {
        // Dated inverse of add_core (used when un-painting at a later date).
        s.cores.delete(mut.value);
      }
    }
  }

  // --- Religion pending projection (base mode-data + queue) -----------------

  /// Base religion state of a province from the loaded mode data (present ⟺ the
  /// province has a religion key at 1444, i.e. a non-NONE group).
  function religionBaseEff(id: number): RelEff {
    const gi = modeData && id < modeData.values.length ? modeData.values[id] : NONE;
    const rel = gi !== NONE ? (modeData!.groups[gi]?.key ?? null) : null;
    return { religion: rel, present: gi !== NONE };
  }
  function religionEff(id: number): RelEff {
    return religionEdited.get(id) ?? religionBaseEff(id);
  }
  function religionMutEff(id: number): RelEff {
    let e = religionEdited.get(id);
    if (!e) {
      e = religionBaseEff(id);
      religionEdited.set(id, e);
    }
    return e;
  }

  /// Folds one queued edit into `religionEdited` (province-history religion key).
  function foldReligionEdit(e: TypedEdit) {
    const m = provinceEditMutations(e);
    if (!m || !m.file.startsWith("history/provinces/")) return;
    const id = idFromFile(m.file);
    if (id === null) return;
    const s = religionMutEff(id);
    for (const mut of m.muts) {
      if (mut.key !== "religion") continue;
      if (mut.remove) {
        s.religion = null;
        s.present = false;
      } else {
        s.religion = mut.value;
        s.present = true;
      }
    }
  }

  // --- Culture pending projection (base mode-data + queue) ------------------

  function cultureBaseEff(id: number): CulEff {
    const gi = modeData && id < modeData.values.length ? modeData.values[id] : NONE;
    const cul = gi !== NONE ? (modeData!.groups[gi]?.key ?? null) : null;
    return { culture: cul, present: gi !== NONE };
  }
  function cultureEff(id: number): CulEff {
    return cultureEdited.get(id) ?? cultureBaseEff(id);
  }
  function cultureMutEff(id: number): CulEff {
    let e = cultureEdited.get(id);
    if (!e) {
      e = cultureBaseEff(id);
      cultureEdited.set(id, e);
    }
    return e;
  }

  /// Folds one queued edit into `cultureEdited` (province-history culture key).
  function foldCultureEdit(e: TypedEdit) {
    const m = provinceEditMutations(e);
    if (!m || !m.file.startsWith("history/provinces/")) return;
    const id = idFromFile(m.file);
    if (id === null) return;
    const s = cultureMutEff(id);
    for (const mut of m.muts) {
      if (mut.key !== "culture") continue;
      if (mut.remove) {
        s.culture = null;
        s.present = false;
      } else {
        s.culture = mut.value;
        s.present = true;
      }
    }
  }

  // --- Trade Goods pending projection (base mode-data + queue) --------------

  function tradeGoodBaseEff(id: number): GoodEff {
    const gi = modeData && id < modeData.values.length ? modeData.values[id] : NONE;
    const good = gi !== NONE ? (modeData!.groups[gi]?.key ?? null) : null;
    return { good, present: gi !== NONE };
  }
  function tradeGoodEff(id: number): GoodEff {
    return tradeGoodEdited.get(id) ?? tradeGoodBaseEff(id);
  }
  function tradeGoodMutEff(id: number): GoodEff {
    let e = tradeGoodEdited.get(id);
    if (!e) {
      e = tradeGoodBaseEff(id);
      tradeGoodEdited.set(id, e);
    }
    return e;
  }

  /// Folds one queued edit into `tradeGoodEdited` (province-history trade_goods).
  function foldTradeGoodEdit(e: TypedEdit) {
    const m = provinceEditMutations(e);
    if (!m || !m.file.startsWith("history/provinces/")) return;
    const id = idFromFile(m.file);
    if (id === null) return;
    const s = tradeGoodMutEff(id);
    for (const mut of m.muts) {
      if (mut.key !== "trade_goods") continue;
      if (mut.remove) {
        s.good = null;
        s.present = false;
      } else {
        s.good = mut.value;
        s.present = true;
      }
    }
  }

  /// Pure fold into an id → good-key overlay (used by the reactive count derive).
  function foldTradeGoodEditInto(overlay: Map<number, string | null>, e: TypedEdit) {
    const m = provinceEditMutations(e);
    if (!m || !m.file.startsWith("history/provinces/")) return;
    const id = idFromFile(m.file);
    if (id === null) return;
    for (const mut of m.muts) {
      if (mut.key !== "trade_goods") continue;
      overlay.set(id, mut.remove ? null : mut.value);
    }
  }

  /// Trade Goods analog of applyPendingToMap: recolor pending-painted provinces
  /// and whole-good recolors (from the editor's ColorPicker / a created good), and
  /// rebuild the per-province icon overlay items.
  function applyTradeGoodPendingToMap() {
    if (!modeData || !compositor) {
      compositor?.setOverrides(new Map());
      rebuildOverlayItems();
      return;
    }
    const overrides = new Map<number, Rgb>();
    for (const [id, e] of tradeGoodEdited) {
      const gi = e.good != null ? (tradeGoodKeyToGroup.get(e.good) ?? NONE) : NONE;
      const baseGi = id < modeData.values.length ? modeData.values[id] : NONE;
      const colorOnly = gi !== NONE && e.good != null && pendingTradeGoodColors.has(e.good);
      if (gi === baseGi && !colorOnly) continue;
      pendingGroup.set(id, gi);
      const col =
        gi === NONE
          ? LAND_RGB
          : (e.good != null ? pendingTradeGoodColors.get(e.good) : undefined) ??
            modeData.groups[gi]?.color ??
            LAND_RGB;
      overrides.set(id, col);
    }
    // Whole-good recolor: every province of a good whose color is pending.
    if (pendingTradeGoodColors.size > 0) {
      const vals = modeData.values;
      for (const [gk, rgb] of pendingTradeGoodColors) {
        const gi = tradeGoodKeyToGroup.get(gk);
        if (gi === undefined) continue;
        for (let id = 0; id < vals.length; id++) {
          if (overrides.has(id)) continue;
          if (effectiveGroup(id) === gi) overrides.set(id, rgb);
        }
      }
    }
    compositor.setOverrides(overrides);
    rebuildOverlayItems();
    updateHighlight();
    redraw();
  }

  /// The list/editor reports a pending map color here so the trade-goods map
  /// repaints live (the edit queue remains the source of truth).
  function onTradeGoodColor(key: string, rgb: [number, number, number] | null) {
    if (rgb) pendingTradeGoodColors.set(key, rgb);
    else pendingTradeGoodColors.delete(key);
    applyPendingToMap();
  }

  /// Rebuilds the per-province trade-good icon overlay items from the effective
  /// (base + pending) good of every land province. Skips unknown/no-good and any
  /// good with no atlas frame (pending scaffolds render nothing until saved).
  function rebuildOverlayItems() {
    if (mode !== "trade_goods" || !modeData || !overlayAtlas) {
      if (overlayItems.size > 0) overlayItems = new Map();
      return;
    }
    const cfg = ICON_OVERLAY_MODES[mode];
    const items = new Map<number, OverlayItem>();
    const vals = modeData.values;
    for (let id = 0; id < vals.length; id++) {
      const e = tradeGoodEdited.get(id);
      const raw = e ? e.good : vals[id] !== NONE ? (modeData.groups[vals[id]]?.key ?? null) : null;
      const key = raw != null ? goodKeyOfGroup(raw) : null;
      if (!key || cfg.skip.has(key)) continue;
      const frame = overlayAtlasIndex.get(key);
      if (frame === undefined) continue;
      items.set(id, { iconIndex: frame });
    }
    overlayItems = items;
  }

  // --- Development pending projection (base bulk data + queue + stroke) ------

  /// Base dev components of a province from the bulk payload (absent key → 0,
  /// present=false; the paint tool creates absent keys at the floor).
  function devBaseEff(id: number): DevEff {
    const b = baseProv?.get(id);
    const g = (v: number | null | undefined): [number, boolean] =>
      v == null ? [0, false] : [v, true];
    const [t, tp] = g(b?.base_tax);
    const [p, pp] = g(b?.base_production);
    const [m, mp] = g(b?.base_manpower);
    return { vals: [t, p, m], present: [tp, pp, mp] };
  }
  function devEff(id: number): DevEff {
    return devEdited.get(id) ?? devBaseEff(id);
  }
  function devMutEff(id: number): DevEff {
    let e = devEdited.get(id);
    if (!e) {
      const b = devBaseEff(id);
      e = { vals: [...b.vals], present: [...b.present] } as DevEff;
      devEdited.set(id, e);
    }
    return e;
  }

  /// Folds one queued edit into `devEdited` (base_tax/production/manpower).
  function foldDevEdit(e: TypedEdit) {
    const m = provinceEditMutations(e);
    if (!m || !m.file.startsWith("history/provinces/")) return;
    const id = idFromFile(m.file);
    if (id === null) return;
    for (const mut of m.muts) {
      const k = DEV_KEYS.indexOf(mut.key as (typeof DEV_KEYS)[number]);
      if (k < 0) continue;
      const s = devMutEff(id);
      if (mut.remove) {
        s.vals[k] = 0;
        s.present[k] = false;
      } else {
        s.vals[k] = Math.max(0, Math.round(parseFloat(mut.value) || 0));
        s.present[k] = true;
      }
    }
  }

  /// Effective (base + queue + live stroke) dev components of a province, plus
  /// whether it carries any dev at all (drives the stat overlay + recolor).
  function effectiveDevComps(id: number): {
    vals: [number, number, number];
    present: [boolean, boolean, boolean];
    any: boolean;
  } {
    const a = devStroke.get(id);
    if (a) {
      const dir: DevDir = armedTool === "dev_lower" ? -1 : 1;
      const vals: [number, number, number] = [
        devValue(a, 0, dir),
        devValue(a, 1, dir),
        devValue(a, 2, dir),
      ];
      const present: [boolean, boolean, boolean] = [
        a.present[0] || a.alloc[0] > 0,
        a.present[1] || a.alloc[1] > 0,
        a.present[2] || a.alloc[2] > 0,
      ];
      return { vals, present, any: true };
    }
    const e = devEff(id);
    return { vals: e.vals, present: e.present, any: e.present.some((x) => x) };
  }

  /// Dev analog of applyPendingToMap: recolor every touched province's gradient
  /// with the client-side dev ramp, and rebuild the per-province stat overlay.
  function applyDevPendingToMap() {
    pendingGroup = new Map();
    if (!modeData || !compositor) {
      compositor?.setOverrides(new Map());
      rebuildDevOverlay();
      return;
    }
    const overrides = new Map<number, Rgb>();
    const touched = new Set<number>([...devEdited.keys(), ...devStroke.keys()]);
    for (const id of touched) {
      const b = baseProv?.get(id);
      if (!b || b.water || b.wasteland) continue;
      const c = effectiveDevComps(id);
      overrides.set(id, devColor(c.vals[0] + c.vals[1] + c.vals[2]));
    }
    compositor.setOverrides(overrides);
    rebuildDevOverlay();
    updateHighlight();
    redraw();
  }

  /// Rebuilds the dev stat-box overlay for every land province with dev data
  /// (9.3). Live-updates while painting because it reads effectiveDevComps.
  function rebuildDevOverlay() {
    if (mode !== "development" || !overlayAtlas || !baseProv) {
      if (overlayItems.size > 0) overlayItems = new Map();
      return;
    }
    const ti = overlayAtlasIndex.get("base_tax") ?? 0;
    const pi = overlayAtlasIndex.get("base_production") ?? 1;
    const mi = overlayAtlasIndex.get("base_manpower") ?? 2;
    const icons = [ti, pi, mi];
    const items = new Map<number, OverlayItem>();
    for (const [id, b] of baseProv) {
      if (b.water || b.wasteland) continue;
      const c = effectiveDevComps(id);
      if (!c.any) continue;
      items.set(id, {
        statBox: [String(c.vals[0]), String(c.vals[1]), String(c.vals[2])],
        statIcons: icons,
      });
    }
    overlayItems = items;
  }

  // --- Trade-detail overlay (S3.3) ------------------------------------------

  /// Folds one queued edit into `cotEdited` (province-history center_of_trade),
  /// so a province-panel CoT change updates the overlay live before save.
  function foldCotEdit(e: TypedEdit) {
    const m = provinceEditMutations(e);
    if (!m || !m.file.startsWith("history/provinces/")) return;
    const id = idFromFile(m.file);
    if (id === null) return;
    for (const mut of m.muts) {
      if (mut.key !== "center_of_trade") continue;
      if (mut.remove) cotEdited.set(id, null);
      else cotEdited.set(id, Math.round(parseFloat(mut.value) || 0));
    }
  }

  /// Effective CoT tier of a province = base (date-folded backend) + pending.
  function effectiveCot(id: number): number | null {
    if (cotEdited.has(id)) return cotEdited.get(id) ?? null;
    return tradeDetailBase.get(id)?.cot ?? null;
  }

  /// Atlas frame for a CoT tier + coastal-ness (inland t → t-1; coastal t → 2+t).
  function cotFrame(tier: number, coastal: boolean): number | undefined {
    if (tier < 1 || tier > 3) return undefined;
    const key = coastal ? `cot_coastal_${tier}` : `cot_inland_${tier}`;
    return overlayAtlasIndex.get(key);
  }

  /// Rebuilds the trade-detail overlay items (CoT icon primary; trade-modifier
  /// badge secondary). A modifier-only province draws the badge as its primary
  /// glyph. Max two glyphs per province; nothing drawn on sea provinces (the
  /// backend never decorates water).
  function rebuildTradeDetailOverlay() {
    if (mode !== "trade_nodes" || !overlayAtlas || !showTradeDetails) {
      if (overlayItems.size > 0) overlayItems = new Map();
      return;
    }
    const modFrame = overlayAtlasIndex.get("trade_modifier");
    const items = new Map<number, OverlayItem>();
    // Union of base-decorated provinces and any with a pending CoT edit.
    const ids = new Set<number>([...tradeDetailBase.keys(), ...cotEdited.keys()]);
    for (const id of ids) {
      const base = tradeDetailBase.get(id);
      const coastal = base?.coastal ?? false;
      const hasMods = (base?.modifiers.length ?? 0) > 0;
      const tier = effectiveCot(id);
      const cotIdx = tier != null ? cotFrame(tier, coastal) : undefined;
      if (cotIdx !== undefined) {
        // CoT primary; modifier badge secondary when present.
        items.set(id, hasMods && modFrame !== undefined
          ? { iconIndex: cotIdx, badgeIndex: modFrame }
          : { iconIndex: cotIdx });
      } else if (hasMods && modFrame !== undefined) {
        // Modifier-only province: the badge is the sole (primary) glyph.
        items.set(id, { iconIndex: modFrame });
      }
    }
    overlayItems = items;
  }

  /// Loads the trade-detail base data for the current date, then rebuilds the
  /// overlay. Called on trade-nodes mode entry and after a save reload.
  async function loadTradeDetails(seq: number, mp: string | null = modPath) {
    try {
      const list = await invoke<TradeDetail[]>("get_trade_details", {
        installPath,
        modPath: mp,
        date: selectedDate,
      });
      if (seq !== renderSeq) return;
      const m = new Map<number, TradeDetail>();
      for (const d of list) m.set(d.id, d);
      tradeDetailBase = m;
    } catch {
      tradeDetailBase = new Map();
    }
    cotEdited = new Map();
    for (const e of visibleHistoryEdits()) foldCotEdit(e);
    rebuildTradeDetailOverlay();
  }

  /// Loads the icon atlas for a mode that has an overlay config (Sprint 7.6).
  async function loadOverlayAtlas(m: string, seq: number, mp: string | null = modPath) {
    const cfg = ICON_OVERLAY_MODES[m];
    if (!cfg) return;
    try {
      const buf = await invoke<ArrayBuffer>("get_icon_atlas", {
        installPath,
        modPath: mp,
        kind: cfg.atlasKind,
      });
      if (seq !== renderSeq) return;
      const parsed = parseAtlasWire(buf);
      const image = await createImageBitmap(new Blob([parsed.png], { type: "image/png" }));
      if (seq !== renderSeq) {
        image.close();
        return;
      }
      overlayAtlas = { image, frameW: parsed.frameW, frameH: parsed.frameH, count: parsed.count };
      overlayAtlasIndex = new Map(Object.entries(parsed.index));
    } catch {
      overlayAtlas = null;
      overlayAtlasIndex = new Map();
    }
  }

  /// Trade Goods brush: paint `trade_goods = <selectedGoodKey>` (a real good or
  /// the "unknown" sentinel) onto land provinces. Single paint tool — no eraser.
  function applyTradeGoodToolTo(id: number): boolean {
    const gk = selectedGoodKey;
    if (!gk || !baseProv) return false;
    const b = baseProv.get(id);
    if (!b || b.water || b.wasteland) return false;
    const s = tradeGoodEff(id);
    if (s.good === gk) return false;
    const e = tradeGoodMutEff(id);
    if (e.present) {
      strokeEdits.push({ kind: "setScalar", file: b.file, path: ["trade_goods"], value: gk, quoted: false });
    } else {
      strokeEdits.push({ kind: "insertStatement", file: b.file, blockPath: [], statement: `trade_goods = ${gk}` });
    }
    e.good = gk;
    e.present = true;
    return true;
  }

  /// Current effective HRE membership of a province (pending overlay over the
  /// backend member set at the selected date).
  function hreEffMember(id: number): boolean {
    return hreEdited.has(id) ? hreEdited.get(id)! : hreMemberCache.has(id);
  }

  /// HRE membership brush (Sprint 29): paint `hre = yes` / `hre = no` on LAND
  /// provinces only (sea/wasteland never painted). Writes the positive scalar so
  /// it composes cleanly into a dated block at a later date (strokeEditsAtDate);
  /// a non-member has no `hre` key so we INSERT, a member has `hre = yes` so we
  /// overwrite with SetScalar. Only flips actual membership (repeat hits no-op).
  function applyHreToolTo(id: number): boolean {
    if (!baseProv) return false;
    const b = baseProv.get(id);
    if (!b || b.water || b.wasteland) return false;
    const want = armedTool === "hre_add";
    if (hreEffMember(id) === want) return false;
    if (want) {
      // Non-member: the `hre` key is absent (vanilla defaults out-of-HRE by
      // absence), so insert it.
      strokeEdits.push({ kind: "insertStatement", file: b.file, blockPath: [], statement: "hre = yes" });
    } else {
      // Member: `hre = yes` is present — overwrite it to no.
      strokeEdits.push({ kind: "setScalar", file: b.file, path: ["hre"], value: "no", quoted: false });
    }
    hreEdited.set(id, want);
    return true;
  }

  /// Folds one queued edit into `hreEdited` (province-history `hre` bool) over the
  /// backend member cache, so the brush preview + dedup reflect the queue.
  function foldHreEdit(e: TypedEdit) {
    const m = provinceEditMutations(e);
    if (!m || !m.file.startsWith("history/provinces/")) return;
    const id = idFromFile(m.file);
    if (id === null) return;
    for (const mut of m.muts) {
      if (mut.key !== "hre") continue;
      hreEdited.set(id, !mut.remove && mut.value.trim() === "yes");
    }
  }

  /// Load the HRE member province set for the brush (political mode only), at the
  /// selected date. Cheap; refreshed when the date changes.
  async function loadHreMembers(install: string, mod: string | null, d: string | null) {
    try {
      const m = await invoke<HreMembers>("get_hre_members", { installPath: install, modPath: mod, date: d });
      hreMemberCache = new Set(m.provinceIds);
    } catch {
      hreMemberCache = new Set();
    }
  }
  $effect(() => {
    if (mode !== "political") return;
    void loadHreMembers(installPath, modPath, selectedDate);
  });

  // --- Trade Goods selection + create (Sprint 7.1/7.2/7.4) ------------------

  /// Select a good from the list (the picker). Highlights its provinces via the
  /// mode-data group (empty when the good has no provinces yet); keeps
  /// `selectedGoodKey` as the paint target regardless.
  function selectGood(key: string) {
    if (selectedGoodKey === key) {
      selectedGoodKey = null;
      select(NONE);
      return;
    }
    selectedGoodKey = key;
    // Undiscovered from the LIST = "everything undiscovered": selectedGroup
    // stays NONE and updateHighlight tints every cluster + absent-good land.
    // (A MAP click on an unknown province instead selects its single cluster
    // via selectGoodByGroup.)
    const gi = key === UNKNOWN_KEY ? undefined : tradeGoodKeyToGroup.get(key);
    select(gi ?? NONE);
  }

  /// Map eyedropper: select the good under the cursor (its list row highlights).
  /// For an undiscovered province the GROUP is its spawn cluster (granular
  /// highlight) while the list row is the base "unknown" entry.
  function selectGoodByGroup(g: number) {
    if (g === NONE || !modeData) return;
    const key = modeData.groups[g]?.key;
    if (key) {
      selectedGoodKey = goodKeyOfGroup(key);
      select(g);
    }
  }

  /// Create-good flow (7.4, no-map-click variant): scaffold the good, queue the
  /// composite, register it pending-side, and auto-select it for painting.
  async function doCreateTradeGood(name: string) {
    if (!modeData) return;
    const pending = createdGoods.map((g) => ({ color: (g.rgb ?? [128, 128, 128]) as Rgb }));
    let scaffold: TradeGoodScaffold;
    try {
      scaffold = await invoke<TradeGoodScaffold>("prepare_trade_good_scaffold", {
        installPath,
        modPath,
        name,
        pending,
      });
    } catch (e) {
      error = String(e);
      return;
    }
    queue.push({
      label: `Create trade good ${name}`,
      edits: scaffold.edits as TypedEdit[],
    });

    const rgb = scaffold.rgb as Rgb;
    // Register a synthetic good for the list + a mode-data group for painting.
    const good: TradeGood = {
      key: scaffold.key,
      index: scaffold.index,
      localizedName: name,
      colorRaw: scaffold.colorFloats,
      colorIsFloat: true,
      rgb,
      basePrice: "1",
      priceFile: PRICES_FILE,
      modifierRows: [],
      provinceRows: [],
      chance: { base_factor: "0", has_conditional_modifiers: false, conditional_count: 0 },
      isLatent: false,
      isValuable: false,
      rawExtra: [],
      sourceFile: GOODS_FILE,
      pending: true,
    };
    createdGoods = [...createdGoods, good];
    const newIndex = modeData.groups.length;
    modeData.groups.push({ key: scaffold.key, label: name, color: rgb });
    tradeGoodKeyToGroup.set(scaffold.key, newIndex);
    selectGood(scaffold.key);
    refreshPending();
  }

  /// The flattened queue edits visible at the selected date (Sprint 12.3): all
  /// date-agnostic composites plus dated composites whose date ≤ selectedDate.
  function visibleHistoryEdits(): TypedEdit[] {
    return queue.serializeVisibleAt(
      (d) => d == null || selectedDate == null || compareDates(d, selectedDate) <= 0,
    );
  }

  /// True when the selected date is strictly after the effective start — the
  /// point at which history writes must go into a dated block, not top level.
  function editingAtLaterDate(): boolean {
    return selectedDate != null && compareDates(selectedDate, effectiveStart) > 0;
  }

  /// Per province, the keys already assigned by dated blocks at or before the
  /// selected date (backend `get_province_shadowed_keys`). A province in here
  /// has history that overrides its top level before the campaign starts, so a
  /// paint stroke must write a dated block rather than the baseline — otherwise
  /// the edit is silently overridden on a timeline mod. Empty on vanilla at
  /// 1444.11.11. Refreshed with the date; a stale/failed load degrades to the
  /// pre-existing top-level behaviour rather than blocking the edit.
  let shadowedKeysByProvince = $state(new Map<number, Set<string>>());

  async function loadShadowedKeys(install: string, mod: string | null, d: string | null) {
    try {
      const raw = await invoke<Record<string, string[]>>("get_province_shadowed_keys", {
        installPath: install,
        modPath: mod,
        date: d,
      });
      const next = new Map<number, Set<string>>();
      for (const [id, keys] of Object.entries(raw)) next.set(Number(id), new Set(keys));
      shadowedKeysByProvince = next;
    } catch {
      shadowedKeysByProvince = new Map();
    }
  }
  $effect(() => {
    void loadShadowedKeys(installPath, modPath, selectedDate);
  });

  /// The dated-block statement for a start-date-shaped stroke edit (paint/dev),
  /// or null when it has no clean dated form (scalar clears / owner unset —
  /// these are only meaningful as base-state edits). `remove_*` inverses cover
  /// core removal so un-painting land at a later date still records something.
  function strokeEditToDatedStatement(e: TypedEdit): string | null {
    if (e.kind === "setScalar") {
      const key = e.path[e.path.length - 1];
      return `${key} = ${e.quoted ? `"${e.value}"` : e.value}`;
    }
    if (e.kind === "insertStatement") return e.statement.trim();
    if (e.kind === "removeStatement") {
      if (e.key === "add_core" && e.value) return `remove_core = ${e.value}`;
      if (e.key === "add_claim" && e.value) return `remove_claim = ${e.value}`;
      return null; // owner/controller/religion/culture/goods have no dated unset
    }
    return null;
  }

  /// Rewrites a stroke's start-date-shaped edits into dated-block edits whenever
  /// the top level is not the state at the selected date — either because the
  /// date is past the start (Sprint 12.3) or because that province's own history
  /// already overrides the painted keys beforehand (timeline mods). All
  /// statements for one province (file) collapse into a single `Y.M.D = { … }`
  /// block so a brush stamp that sets owner+controller+core writes one block,
  /// not three. Edits on non-province files, and provinces whose baseline is
  /// still authoritative, pass through unchanged.
  function strokeEditsAtDate(edits: TypedEdit[]): TypedEdit[] {
    if (selectedDate == null) return edits;
    const date = selectedDate;
    const byFile = new Map<string, { edits: TypedEdit[]; statements: string[] }>();
    const passthrough: TypedEdit[] = [];
    for (const e of edits) {
      const file = "file" in e ? (e.file as string) : undefined;
      const stmt = file?.startsWith("history/provinces/")
        ? strokeEditToDatedStatement(e)
        : null;
      if (file && stmt) {
        const g = byFile.get(file) ?? { edits: [], statements: [] };
        g.edits.push(e);
        g.statements.push(stmt);
        byFile.set(file, g);
      } else {
        passthrough.push(e);
      }
    }
    const out = [...passthrough];
    for (const [file, g] of byFile) {
      const id = idFromFile(file);
      out.push(
        ...editAtDate({
          file,
          selectedDate: date,
          startDate: effectiveStart,
          // Paint has no per-province dated-block list loaded, so a dated write
          // always inserts a fresh block (valid EU4; duplicate-date blocks are
          // legal and mod_writer places it in date order).
          datedBlocks: [],
          startEdits: g.edits,
          statements: g.statements,
          shadowedKeys:
            (id !== null ? shadowedKeysByProvince.get(id) : undefined) ?? new Set<string>(),
        }),
      );
    }
    return out;
  }

  /// True when `edits` contains at least one province-history write that
  /// `strokeEditsAtDate` will turn into a dated block — the condition for
  /// date-tagging the composite so the pending folds gate it.
  function strokeWritesDatedBlock(edits: TypedEdit[]): boolean {
    if (selectedDate == null) return false;
    if (editingAtLaterDate()) {
      return edits.some(
        (e) => "file" in e && (e.file as string).startsWith("history/provinces/"),
      );
    }
    return edits.some((e) => {
      if (!("file" in e)) return false;
      const file = e.file as string;
      if (!file.startsWith("history/provinces/")) return false;
      const stmt = strokeEditToDatedStatement(e);
      if (stmt === null) return false;
      const id = idFromFile(file);
      const keys = id !== null ? shadowedKeysByProvince.get(id) : undefined;
      return keys != null && isShadowed([stmt], keys);
    });
  }

  /// Pushes a stroke composite through the date rule. Province-history strokes
  /// (owner/controller/cores, religion, culture, goods, dev) are rewritten into
  /// dated blocks and the composite is date-tagged so the pending folds gate it.
  /// Static-file strokes (climate/terrain/geo/trade-node membership) are NOT
  /// date-aware in EU4 — they pass through untagged and always apply. At the
  /// start date every stroke is an ordinary push.
  function pushStroke(label: string, edits: TypedEdit[]) {
    const dated = strokeWritesDatedBlock(edits);
    const finalEdits = dated ? strokeEditsAtDate(edits) : edits;
    if (finalEdits.length === 0) return;
    queue.push({ label, edits: finalEdits, ...(dated ? { date: selectedDate! } : {}) });
  }

  /// Rebuilds the mode-relevant pending projection from the committed queue and
  /// repaints. Called on queue.version changes and after a mode load.
  function refreshPending() {
    if (mode === "trade_nodes") {
      applyTradeNodePendingToMap();
      // Fold pending CoT edits (province panel) so the overlay updates live.
      cotEdited = new Map();
      for (const e of visibleHistoryEdits()) foldCotEdit(e);
      rebuildTradeDetailOverlay();
      return;
    }
    if (mode === "areas" || mode === "regions") {
      applyGeoPendingToMap();
      return;
    }
    if (isColonialMode) {
      applyColonialPendingToMap();
      return;
    }
    if (mode === "climate" || mode === "winter") {
      applyClimatePendingToMap();
      return;
    }
    if (mode === "simple_terrain") {
      terrainOverlay = new Map();
      for (const e of queue.serialize()) foldTerrainEditInto(terrainOverlay, e);
      applyPendingToMap();
      return;
    }
    // Date-aware history folds see only composites made at a date ≤ the selected
    // view date (Sprint 12.3): an edit made at 1444 disappears when viewing 1300
    // but stays in the queue and still saves. Static-file folds (terrain above,
    // climate/geo/trade-node below) don't gate — their composites carry no date.
    const datedEdits = visibleHistoryEdits();
    if (mode === "trade_goods") {
      tradeGoodEdited = new Map();
      for (const e of datedEdits) foldTradeGoodEdit(e);
      applyPendingToMap();
      return;
    }
    if (mode === "development") {
      devEdited = new Map();
      for (const e of datedEdits) foldDevEdit(e);
      applyPendingToMap();
      return;
    }
    if (mode === "religion") {
      religionEdited = new Map();
      for (const e of datedEdits) foldReligionEdit(e);
    } else if (mode === "culture") {
      cultureEdited = new Map();
      for (const e of datedEdits) foldCultureEdit(e);
    } else {
      edited = new Map();
      for (const e of datedEdits) foldEdit(e);
      // HRE membership brush overlay (Sprint 29): pending hre toggles over the
      // backend member cache, for the brush preview + Members count.
      hreEdited = new Map();
      for (const e of datedEdits) foldHreEdit(e);
    }
    applyPendingToMap();
  }

  /// Derives `pendingGroup` + recolor overrides from `edited` and pushes them
  /// into the compositor. Only meaningful in political mode.
  function applyPendingToMap() {
    pendingGroup = new Map();
    if (!modeData || !compositor) {
      compositor?.setOverrides(new Map());
      return;
    }
    if (mode === "religion") {
      applyReligionPendingToMap();
      return;
    }
    if (mode === "culture") {
      applyCulturePendingToMap();
      return;
    }
    if (mode === "trade_nodes") {
      applyTradeNodePendingToMap();
      return;
    }
    if (mode === "areas" || mode === "regions") {
      applyGeoPendingToMap();
      return;
    }
    if (isColonialMode) {
      applyColonialPendingToMap();
      return;
    }
    if (mode === "climate" || mode === "winter") {
      applyClimatePendingToMap();
      return;
    }
    if (mode === "simple_terrain") {
      applyTerrainPendingToMap();
      return;
    }
    if (mode === "trade_goods") {
      applyTradeGoodPendingToMap();
      return;
    }
    if (mode === "development") {
      applyDevPendingToMap();
      return;
    }
    if (mode !== "political") {
      compositor.setOverrides(new Map());
      return;
    }
    // Base occupation stripes baked into the render (id -> controller color); the
    // pending fold compares against these so a restored/undone occupation reverts.
    const baseStripe = new Map<number, Rgb>();
    for (const st of modeData.stripes) baseStripe.set(st.id, st.color);

    const overrides = new Map<number, Override>();
    for (const [id, e] of edited) {
      const gi = e.owner != null ? (tagToGroup.get(e.owner) ?? NONE) : NONE;
      const baseGi = id < modeData.values.length ? modeData.values[id] : NONE;
      // A pending color on the (unchanged) owner is still a visual change.
      const colorOnly = gi !== NONE && e.owner != null && pendingColors.has(e.owner);
      // Effective occupation: controller set, differs from owner (owner present).
      const occupied = e.owner != null && e.controller != null && e.controller !== e.owner;
      const ctrlColor = occupied ? controllerStripeColor(e.controller!) : null;
      const priorStripe = baseStripe.get(id) ?? null;
      const stripeChanged = !rgbEq(ctrlColor, priorStripe);
      if (gi === baseGi && !colorOnly && !stripeChanged) continue;
      pendingGroup.set(id, gi); // hit-testing stays owner-based
      const ownerColor =
        gi === NONE
          ? LAND_RGB
          : (e.owner != null ? pendingColors.get(e.owner) : undefined) ??
            modeData.groups[gi].color;
      overrides.set(id, occupied ? { fill: ownerColor, stripe: ctrlColor! } : ownerColor);
    }
    // Whole-country recolor: every province of a tag whose color is pending. Keep
    // an occupied province striped (its base controller color as the stripe).
    if (pendingColors.size > 0) {
      const vals = modeData.values;
      for (const [t, rgb] of pendingColors) {
        const gi = tagToGroup.get(t);
        if (gi === undefined) continue;
        for (let id = 0; id < vals.length; id++) {
          if (overrides.has(id)) continue;
          if (effectiveGroup(id) === gi) {
            const st = baseStripe.get(id);
            overrides.set(id, st ? { fill: rgb, stripe: st } : rgb);
          }
        }
      }
    }
    compositor.setOverrides(overrides);
    updateHighlight();
    redraw();
  }

  /// The stripe color for an occupying controller: rebel gray for `REB`, else the
  /// controller country's (pending-aware) political color; falls back to rebel
  /// gray for a controller with no political group (shouldn't normally occur).
  function controllerStripeColor(tag: string): Rgb {
    if (tag === "REB") return REBEL_GRAY;
    const gi = tagToGroup.get(tag);
    if (gi === undefined) return REBEL_GRAY;
    return pendingColors.get(tag) ?? modeData!.groups[gi].color;
  }

  /// RGB equality with null handling (both null = equal).
  function rgbEq(a: Rgb | null, b: Rgb | null): boolean {
    if (a === null || b === null) return a === b;
    return a[0] === b[0] && a[1] === b[1] && a[2] === b[2];
  }

  /// The country panel reports its pending map color here so the political map
  /// repaints live (the edit queue remains the source of truth).
  function onCountryColor(t: string, rgb: [number, number, number] | null) {
    if (rgb) pendingColors.set(t, rgb);
    else pendingColors.delete(t);
    applyPendingToMap();
  }

  /// Religion analog of applyPendingToMap: recolor pending-painted provinces and
  /// whole-religion recolors (from the panel's ColorPicker / a created religion).
  function applyReligionPendingToMap() {
    if (!modeData || !compositor) return;
    const overrides = new Map<number, Rgb>();
    for (const [id, e] of religionEdited) {
      const gi = e.religion != null ? (religionKeyToGroup.get(e.religion) ?? NONE) : NONE;
      const baseGi = id < modeData.values.length ? modeData.values[id] : NONE;
      const colorOnly =
        gi !== NONE && e.religion != null && pendingReligionColors.has(e.religion);
      if (gi === baseGi && !colorOnly) continue;
      pendingGroup.set(id, gi);
      const col =
        gi === NONE
          ? LAND_RGB
          : (e.religion != null ? pendingReligionColors.get(e.religion) : undefined) ??
            modeData.groups[gi]?.color ??
            LAND_RGB;
      overrides.set(id, col);
    }
    // Whole-religion recolor: every province of a religion whose color is pending.
    if (pendingReligionColors.size > 0) {
      const vals = modeData.values;
      for (const [rk, rgb] of pendingReligionColors) {
        const gi = religionKeyToGroup.get(rk);
        if (gi === undefined) continue;
        for (let id = 0; id < vals.length; id++) {
          if (overrides.has(id)) continue;
          if (effectiveGroup(id) === gi) overrides.set(id, rgb);
        }
      }
    }
    compositor.setOverrides(overrides);
    updateHighlight();
    redraw();
  }

  /// The religion panel reports its pending map color here so the religion map
  /// repaints live (the edit queue remains the source of truth).
  function onReligionColor(key: string, rgb: [number, number, number] | null) {
    if (rgb) pendingReligionColors.set(key, rgb);
    else pendingReligionColors.delete(key);
    applyPendingToMap();
  }

  /// Culture analog of applyPendingToMap: recolor pending-painted provinces and
  /// whole-culture recolors (display-color overrides). Cultures have no color in
  /// the render, so overrides are painted on top of the hash-colored PNG.
  function applyCulturePendingToMap() {
    if (!modeData || !compositor) return;
    const overrides = new Map<number, Rgb>();
    for (const [id, e] of cultureEdited) {
      const gi = e.culture != null ? (cultureKeyToGroup.get(e.culture) ?? NONE) : NONE;
      const baseGi = id < modeData.values.length ? modeData.values[id] : NONE;
      const colorOnly =
        gi !== NONE && e.culture != null && cultureColorOverrides.has(e.culture);
      if (gi === baseGi && !colorOnly) continue;
      pendingGroup.set(id, gi);
      const col =
        gi === NONE
          ? LAND_RGB
          : (e.culture != null ? cultureColorOverrides.get(e.culture) : undefined) ??
            modeData.groups[gi]?.color ??
            LAND_RGB;
      overrides.set(id, col);
    }
    // Whole-culture recolor: every province of a culture with a display override.
    if (cultureColorOverrides.size > 0) {
      const vals = modeData.values;
      for (const [ck, rgb] of cultureColorOverrides) {
        const gi = cultureKeyToGroup.get(ck);
        if (gi === undefined) continue;
        for (let id = 0; id < vals.length; id++) {
          if (overrides.has(id)) continue;
          if (effectiveGroup(id) === gi) overrides.set(id, rgb);
        }
      }
    }
    compositor.setOverrides(overrides);
    updateHighlight();
    redraw();
  }

  /// The culture panel reports a display-color override here so the culture map
  /// repaints live. The override is persisted to the toolkit DB by the panel;
  /// this map is the session's live source of truth for recolor.
  function onCultureColor(key: string, rgb: [number, number, number] | null) {
    if (rgb) cultureColorOverrides.set(key, rgb);
    else cultureColorOverrides.delete(key);
    // Reflect the new color in the group metadata (label swatch + paint default).
    const gi = cultureKeyToGroup.get(key);
    if (gi !== undefined && modeData?.groups[gi]) {
      // On clear we can't recover the exact hash color client-side; the province
      // pixels revert to the hash color already baked into the PNG (no override).
      if (rgb) modeData.groups[gi].color = rgb;
    }
    applyPendingToMap();
  }

  /// Trade Nodes analog of applyPendingToMap: recolor provinces whose (effective)
  /// node membership or node color changed, folding the in-progress paint stroke.
  function applyTradeNodePendingToMap() {
    pendingGroup = new Map();
    if (!modeData || !compositor) {
      compositor?.setOverrides(new Map());
      return;
    }
    const net = tradeNetwork;
    if (!net) {
      compositor.setOverrides(new Map());
      updateHighlight();
      redraw();
      return;
    }
    // province -> effective node key (fold), then the live stroke overlay.
    const eff = membershipIndex(net);
    tnMemberCache = eff;
    // Nodes whose effective color differs from the base color (whole-node repaint).
    const baseColor = new Map<string, string>();
    if (baseNetwork) {
      for (const n of baseNetwork.nodes) if (n.color) baseColor.set(n.key, n.color.join(","));
    }
    const nodeColor = new Map<string, Rgb>();
    const colorChanged = new Set<string>();
    for (const n of net.nodes) {
      const gi = tradeNodeKeyToGroup.get(n.key);
      const col = (n.color ?? (gi !== undefined ? modeData.groups[gi]?.color : undefined)) as
        | Rgb
        | undefined;
      if (col) nodeColor.set(n.key, col);
      const now = n.color ? n.color.join(",") : "";
      if (now && now !== (baseColor.get(n.key) ?? "")) colorChanged.add(n.key);
    }

    const overrides = new Map<number, Rgb>();
    const vals = modeData.values;
    for (let id = 0; id < vals.length; id++) {
      const baseGi = vals[id];
      const baseKey = baseGi !== NONE ? (modeData.groups[baseGi]?.key ?? null) : null;
      let effKey: string | null = tnPaintOverride.has(id)
        ? tnPaintOverride.get(id)!
        : (eff.get(id) ?? null);
      const membershipChanged = effKey !== baseKey;
      const colChanged = effKey != null && colorChanged.has(effKey);
      if (!membershipChanged && !colChanged) continue;
      const gi = effKey != null ? (tradeNodeKeyToGroup.get(effKey) ?? NONE) : NONE;
      pendingGroup.set(id, gi);
      const col =
        gi === NONE
          ? LAND_RGB
          : (effKey != null ? nodeColor.get(effKey) : undefined) ??
            modeData.groups[gi]?.color ??
            LAND_RGB;
      overrides.set(id, col);
    }
    compositor.setOverrides(overrides);
    updateHighlight();
    redraw();
  }

  /// Areas/Regions analog: recolor provinces whose effective area/region
  /// membership changed, folding the in-progress paint stroke. Areas key on the
  /// province directly; regions key on the province's area's region.
  function applyGeoPendingToMap() {
    pendingGroup = new Map();
    if (!modeData || !compositor) {
      compositor?.setOverrides(new Map());
      return;
    }
    const net = geoNetwork;
    if (!net) {
      compositor.setOverrides(new Map());
      updateHighlight();
      redraw();
      return;
    }
    const eff = mode === "areas" ? areaMembershipIndex(net) : regionMembershipIndex(net);
    geoMemberCache = eff;
    const keyToGroup = mode === "areas" ? areaKeyToGroup : regionKeyToGroup;
    const overrides = new Map<number, Rgb>();
    const vals = modeData.values;
    for (let id = 0; id < vals.length; id++) {
      const baseGi = vals[id];
      const baseKey = baseGi !== NONE ? (modeData.groups[baseGi]?.key ?? null) : null;
      const effKey: string | null = geoPaintOverride.has(id)
        ? geoPaintOverride.get(id)!
        : (eff.get(id) ?? null);
      if (effKey === baseKey) continue;
      const gi = effKey != null ? (keyToGroup.get(effKey) ?? NONE) : NONE;
      pendingGroup.set(id, gi);
      const col = gi === NONE ? LAND_RGB : modeData.groups[gi]?.color ?? LAND_RGB;
      overrides.set(id, col);
    }
    compositor.setOverrides(overrides);
    updateHighlight();
    redraw();
  }

  // --- Colonial Regions / Trade Companies (Sprint 19) ----------------------

  function sameRgb(a: Rgb, b: Rgb): boolean {
    return a[0] === b[0] && a[1] === b[1] && a[2] === b[2];
  }

  /// Recolor provinces whose effective colonial-entry membership changed (or
  /// whose entry's color was edited), folding the in-progress paint stroke.
  function applyColonialPendingToMap() {
    pendingGroup = new Map();
    if (!modeData || !compositor) {
      compositor?.setOverrides(new Map());
      return;
    }
    const data = colonialData;
    if (!data) {
      compositor.setOverrides(new Map());
      updateHighlight();
      redraw();
      return;
    }
    const eff = colonialMembershipIndex(data);
    colonialMemberCache = eff;
    // Effective color per entry (may have a pending color edit) → override table.
    const colorByKey = new Map<string, Rgb>();
    for (const e of data.entries) colorByKey.set(e.key, e.color as Rgb);
    const overrides = new Map<number, Rgb>();
    const vals = modeData.values;
    for (let id = 0; id < vals.length; id++) {
      const baseGi = vals[id];
      const baseKey = baseGi !== NONE ? (modeData.groups[baseGi]?.key ?? null) : null;
      const baseColor = baseGi !== NONE ? modeData.groups[baseGi]?.color ?? null : null;
      const effKey: string | null = geoPaintOverride.has(id)
        ? geoPaintOverride.get(id)!
        : (eff.get(id) ?? null);
      const effColor = effKey != null ? (colorByKey.get(effKey) ?? LAND_RGB) : LAND_RGB;
      // Only override when membership OR the entry's color differs from the render.
      const membershipChanged = effKey !== baseKey;
      const colorChanged =
        effKey != null && baseColor != null && effKey === baseKey && !sameRgb(effColor, baseColor);
      if (!membershipChanged && !colorChanged) continue;
      pendingGroup.set(id, effKey != null ? (colonialKeyToGroup.get(effKey) ?? NONE) : NONE);
      overrides.set(id, effKey != null ? effColor : LAND_RGB);
    }
    compositor.setOverrides(overrides);
    updateHighlight();
    redraw();
  }

  /// Colonial membership brush (province granularity) with steal semantics.
  function applyColonialToolTo(id: number): boolean {
    const entryKey = selectedColonialKey;
    if (!entryKey || !colonialData) return false;
    const b = baseProv?.get(id);
    if (!b || b.water || b.wasteland) return false;
    const data = colonialData;
    const ent = data.entries.find((e) => e.key === entryKey);
    const toFile = ent?.source_file || data.project_file;
    const cur = geoPaintOverride.has(id) ? geoPaintOverride.get(id)! : (colonialMemberCache.get(id) ?? null);
    if (armedTool === "col_add") {
      if (cur === entryKey) return false;
      if (cur == null) {
        strokeEdits.push({ kind: "addId", file: toFile, listPath: [entryKey, "provinces"], id: String(id) });
      } else {
        const fromEnt = data.entries.find((e) => e.key === cur);
        strokeEdits.push({
          kind: "listMove",
          fromFile: fromEnt?.source_file || data.project_file,
          fromPath: [cur, "provinces"],
          toFile,
          toPath: [entryKey, "provinces"],
          id: String(id),
        });
      }
      geoPaintOverride.set(id, entryKey);
      return true;
    }
    if (armedTool === "col_remove") {
      if (cur !== entryKey) return false;
      strokeEdits.push({ kind: "removeId", file: toFile, listPath: [entryKey, "provinces"], id: String(id) });
      geoPaintOverride.set(id, null);
      return true;
    }
    return false;
  }

  async function runColonialValidation() {
    if (!isColonialMode) return;
    try {
      colonialIssues = await invoke<ValidationIssue[]>("validate", {
        domain: mode,
        installPath,
        modPath,
      });
    } catch {
      colonialIssues = [];
    }
  }

  /// Issues scoped to the selected colonial entry (mirrors the geo panel filter).
  let selectedColonialIssues = $derived<ValidationIssue[]>(
    (() => {
      const k = selectedColonialKey;
      if (!k) return colonialIssues;
      return colonialIssues.filter(
        (i) =>
          i.jump != null &&
          (i.jump.kind === "colonial_region" || i.jump.kind === "trade_company") &&
          i.jump.id === k,
      );
    })(),
  );

  function colonialJump(j: JumpTarget) {
    if (j.kind === "province") openProvince(j.id);
    else if (j.kind === "colonial_region" || j.kind === "trade_company") {
      openView({ kind: "colonial", colonialKind: j.kind === "colonial_region" ? "colonial_regions" : "trade_companies", key: j.id }, "reuse");
    }
  }

  function onColonialDeleted() {
    select(NONE);
  }

  function registerColonialGroup(key: string, name: string, color: Rgb): number {
    const gi = modeData!.groups.length;
    modeData!.groups.push({ key, label: name, color });
    colonialKeyToGroup.set(key, gi);
    return gi;
  }

  function colonialKeyExists(k: string): boolean {
    return (
      colonialKeyToGroup.has(k) ||
      (baseColonial?.entries.some((e) => e.key === k) ?? false)
    );
  }

  async function doCreateColonial(provinceId: number, name: string) {
    if (!modeData || !colonialData) return;
    const kind = mode; // "colonial_regions" | "trade_companies"
    const prefix = kind === "colonial_regions" ? "colonial" : "trade_company";
    const key = colonialUniqueKey(`${prefix}_${colonialSlugify(name)}`, colonialKeyExists);
    const nameKey = colonialNameLocKey(key);
    // Steal the starting province from its current colonial entry, if any.
    const from = colonialMemberCache.get(provinceId) ?? colonialMembershipIndex(colonialData).get(provinceId) ?? null;
    let stmt: string;
    try {
      stmt = await invoke<string>("scaffold_colonial_block", {
        kind,
        key,
        provinces: [provinceId],
        nameKey,
      });
    } catch (e) {
      error = String(e);
      return;
    }
    const projectFile = colonialData.project_file;
    const edits: TypedEdit[] = [];
    if (from) {
      const fromEnt = colonialData.entries.find((e) => e.key === from);
      edits.push({
        kind: "removeId",
        file: fromEnt?.source_file || projectFile,
        listPath: [from, "provinces"],
        id: String(provinceId),
      });
    }
    edits.push({ kind: "appendText", file: projectFile, text: stmt });
    edits.push({ kind: "locOverride", key: nameKey, value: name });
    queue.push({ label: `Create ${prefix} ${name}`, edits });
    selectedGroup = registerColonialGroup(key, name, colonialHashColor(key));
    refreshPending();
  }
  function acceptColonialName(name: string) {
    const args = colonialFlow.submitName(name || "New Colonial Region");
    if (args) doCreateColonial(args.provinceId, args.name);
    armedTool = null;
    colonialFlow.reset();
  }
  function cancelColonialFlow() {
    armedTool = null;
    colonialFlow.cancel();
    colonialFlow.reset();
  }
  function tryCreateColonialClick(clientX: number, clientY: number): boolean {
    if (colonialFlowState.phase !== "awaiting-click") return false;
    const id = provinceIdAt(clientX, clientY);
    if (id === NONE) return true;
    const b = baseProv?.get(id);
    if (b && (b.water || b.wasteland)) return true; // stay armed on non-land
    const rect = canvas.getBoundingClientRect();
    colonialFlow.mapClicked(id, { x: clientX - rect.left, y: clientY - rect.top });
    return true;
  }

  // --- Climate mode (Sprint 11.1) ------------------------------------------

  /// Effective list key of province `id` in `slot`, honoring the in-progress
  /// paint override (only meaningful for the slot currently being painted).
  function climateCur(id: number, slot: ClimateSlot): string | null {
    if (climateSelSlot === slot && climatePaintOverride.has(id)) {
      return climatePaintOverride.get(id)!;
    }
    return climateModel ? climateKey(climateModel, slot, id) : null;
  }

  /// Recolor the climate/winter map from the effective model. The active mode's
  /// slot drives the base color; in climate mode an optional winter tint overlays
  /// severity. Only provinces whose color differs from the base render are set.
  function applyClimatePendingToMap() {
    pendingGroup = new Map();
    if (!compositor || !climateModel || !baseClimate || !modeData) {
      compositor?.setOverrides(new Map());
      return;
    }
    const activeSlot: ClimateSlot = mode === "climate" ? "zone" : "winter";
    // Base render membership (pre-pending) for the active slot.
    const baseActive = new Map<number, string>(
      (activeSlot === "zone" ? baseClimate.zones : baseClimate.winters).map((e) => [e.id, e.key]),
    );
    const overrides = new Map<number, Rgb>();
    const len = modeData.values.length;
    const tint = mode === "climate" && showWinterTint;
    for (let id = 0; id < len; id++) {
      const b = baseProv?.get(id);
      if (!b || b.water || b.wasteland) continue;
      const effActive = climateCur(id, activeSlot);
      // Desired color for this province given effective state (+ optional tint).
      let want: Rgb;
      if (mode === "climate") {
        const zc = zoneColor(effActive);
        if (tint) {
          const w = climateCur(id, "winter");
          want = w ? blend(zc, winterColor(w), 0.5) : zc;
        } else {
          want = zc;
        }
      } else {
        want = winterColor(effActive);
      }
      // The base render color (no tint) — override only when the pixel differs.
      const baseKey = baseActive.get(id) ?? null;
      const baseCol = mode === "climate" ? zoneColor(baseKey) : winterColor(baseKey);
      const tintChanged = tint && climateCur(id, "winter") != null;
      if (!tintChanged && want[0] === baseCol[0] && want[1] === baseCol[1] && want[2] === baseCol[2]) {
        continue;
      }
      overrides.set(id, want);
    }
    compositor.setOverrides(overrides);
    updateClimateHighlight();
    redraw();
  }

  /// Climate highlight: the selected entry's provinces (SELECTED), the hovered
  /// active-slot group (HOVER), and the brush preview (PREVIEW, wins).
  function updateClimateHighlight() {
    if (!compositor || !modeData || !climateModel) return;
    provinceFill.fill(0);
    const activeSlot: ClimateSlot = mode === "climate" ? "zone" : "winter";
    const len = modeData.values.length;
    for (let id = 0; id < len; id++) {
      if (previewSet.has(id)) {
        provinceFill[id] = PREVIEW_FILL;
        continue;
      }
      const b = baseProv?.get(id);
      if (!b || b.water || b.wasteland) continue;
      // Selected entry (any slot) wins over hover.
      if (climateSelSlot !== null) {
        const cur = climateCur(id, climateSelSlot);
        if (cur === climateSelKey) {
          provinceFill[id] = SELECTED_FILL;
          continue;
        }
      }
      if (climateHovering && climateHoverKey !== null && climateCur(id, activeSlot) === climateHoverKey) {
        provinceFill[id] = HOVER_FILL;
      }
    }
    compositor.setHighlight(provinceFill);
  }

  /// Climate hover (no brush): highlight the active-slot group under the cursor.
  function climateHover(clientX: number, clientY: number) {
    const id = provinceIdAt(clientX, clientY);
    const b = id !== NONE ? baseProv?.get(id) : undefined;
    const activeSlot: ClimateSlot = mode === "climate" ? "zone" : "winter";
    const key = b && !b.water && !b.wasteland ? climateCur(id, activeSlot) : null;
    if (key === climateHoverKey && climateHovering === (key !== null)) return;
    climateHoverKey = key;
    climateHovering = key !== null;
    hovering = climateHovering;
    updateClimateHighlight();
    redraw();
  }

  /// Select a climate selector entry (from the panel or a map eyedropper click).
  function selectClimateEntry(slot: ClimateSlot, key: string | null) {
    if (climateSelSlot === slot && climateSelKey === key) {
      climateSelSlot = null;
      climateSelKey = null;
    } else {
      climateSelSlot = slot;
      climateSelKey = key;
    }
    updateClimateHighlight();
    redraw();
  }

  /// Climate brush: paint the selected entry onto province `id`, stealing within
  /// the selected slot only (the other slot is never touched). Eraser entries
  /// (null key) remove the province from that slot. Land only.
  function applyClimateToolTo(id: number): boolean {
    if (climateSelSlot === null || !climateModel) return false;
    const b = baseProv?.get(id);
    if (!b || b.water || b.wasteland) return false;
    const slot = climateSelSlot;
    const target = climateSelKey; // null = eraser
    const file = climateModel.file;
    const cur = climatePaintOverride.has(id)
      ? climatePaintOverride.get(id)!
      : climateKey(climateModel, slot, id);
    if (target === null) {
      if (cur == null) return false;
      strokeEdits.push({ kind: "removeId", file, listPath: [cur], id: String(id) });
      climatePaintOverride.set(id, null);
      return true;
    }
    if (cur === target) return false;
    // Create the target list block if it doesn't exist yet (base + this stroke).
    if (!climateModel.existingLists.has(target) && !climateInsertedThisStroke.has(target)) {
      strokeEdits.push({ kind: "insertStatement", file, blockPath: [], statement: `${target} = { }` });
      climateInsertedThisStroke.add(target);
    }
    if (cur == null) {
      strokeEdits.push({ kind: "addId", file, listPath: [target], id: String(id) });
    } else {
      strokeEdits.push({
        kind: "listMove",
        fromFile: file,
        fromPath: [cur],
        toFile: file,
        toPath: [target],
        id: String(id),
      });
    }
    climatePaintOverride.set(id, target);
    return true;
  }

  // --- Simple Terrain mode (Sprint 11.2) -----------------------------------

  /// Effective terrain category key of province `id` = pending override overlay
  /// (a category, or AUTO_KEY → the bmp auto class) else the base mode-data group.
  function terrainEffKey(id: number): string | null {
    const ov = terrainOverlay.get(id);
    if (ov !== undefined) {
      return ov === AUTO_KEY ? (terrainAuto.get(id) ?? null) : ov;
    }
    if (!modeData) return null;
    const gi = id < modeData.values.length ? modeData.values[id] : NONE;
    return gi !== NONE ? (modeData.groups[gi]?.key ?? null) : null;
  }

  /// Whether province `id` currently carries a terrain_override (base + pending).
  function terrainIsOverride(id: number): boolean {
    const ov = terrainOverlay.get(id);
    if (ov !== undefined) return ov !== AUTO_KEY;
    return terrainOverrideBase.get(id) ?? false;
  }

  /// Terrain analog of applyPendingToMap: recolor provinces whose effective
  /// terrain changed vs the base render, and rebuild the hit-test overlay.
  function applyTerrainPendingToMap() {
    pendingGroup = new Map();
    if (!compositor || !modeData) {
      compositor?.setOverrides(new Map());
      return;
    }
    const overrides = new Map<number, Rgb>();
    for (const [id] of terrainOverlay) {
      const effKey = terrainEffKey(id);
      const gi = effKey != null ? (terrainKeyToGroup.get(effKey) ?? NONE) : NONE;
      const baseGi = id < modeData.values.length ? modeData.values[id] : NONE;
      if (gi === baseGi) continue;
      pendingGroup.set(id, gi);
      overrides.set(id, gi === NONE ? LAND_RGB : modeData.groups[gi]?.color ?? LAND_RGB);
    }
    compositor.setOverrides(overrides);
    updateHighlight();
    redraw();
  }

  /// Select a terrain category (or the AUTO eraser) from the list. Highlights its
  /// effective provinces via the mode-data group; keeps it as the paint target.
  function selectTerrain(key: string) {
    if (selectedTerrainKey === key) {
      selectedTerrainKey = null;
      select(NONE);
      return;
    }
    selectedTerrainKey = key;
    // The AUTO eraser has no group; highlight is handled by the shared path only
    // for real categories.
    const gi = key === AUTO_KEY ? undefined : terrainKeyToGroup.get(key);
    select(gi ?? NONE);
  }

  /// Commit edited gameplay modifiers for a terrain category (S2.7). Diffs the
  /// row set against the on-disk category (byte-surgical set/insert/remove of only
  /// the modeled keys) and pushes one coalescing composite per category.
  function commitTerrainModifiers(cat: TerrainCategory, rows: ModifierRow[]) {
    // Diff against the ON-DISK category (not the pending-folded one passed in) so
    // the composite fully expresses base→desired and coalescing stays correct.
    const base = baseTerrain?.categories.find((c) => c.key === cat.key);
    if (!base) return;
    queue.push({
      label: `Edit ${cat.key} terrain`,
      edits: terrainModifierEdits(base, rows),
      coalesceKey: `terrainmod:${cat.key}`,
    });
  }

  /// Terrain brush: paint the selected category as a terrain_override onto land
  /// provinces (steal between override lists), or the AUTO eraser removes the
  /// override so the province reverts to its terrain.bmp class.
  function applyTerrainToolTo(id: number): boolean {
    const key = selectedTerrainKey;
    if (!key || !baseProv) return false;
    const b = baseProv.get(id);
    if (!b || b.water || b.wasteland) return false;
    const curOverride = terrainOverlay.get(id);
    const isOverride = terrainIsOverride(id);
    const curOverrideCat = isOverride ? terrainEffKey(id) : null;

    if (key === AUTO_KEY) {
      // Erase: only meaningful when currently overridden.
      if (!isOverride || curOverrideCat == null) return false;
      strokeEdits.push({
        kind: "removeId",
        file: TERRAIN_FILE,
        listPath: ["categories", curOverrideCat, "terrain_override"],
        id: String(id),
      });
      terrainOverlay.set(id, AUTO_KEY);
      return true;
    }

    // Paint a real category; skip when it is already the effective override.
    if (isOverride && curOverrideCat === key) return false;
    // Create the target override block if absent (base has no block for this cat).
    if (!terrainBlockExists(key) && !terrainInsertedThisStroke.has(key)) {
      strokeEdits.push({
        kind: "insertStatement",
        file: TERRAIN_FILE,
        blockPath: ["categories", key],
        statement: "terrain_override = { }",
      });
      terrainInsertedThisStroke.add(key);
    }
    if (isOverride && curOverrideCat) {
      strokeEdits.push({
        kind: "listMove",
        fromFile: TERRAIN_FILE,
        fromPath: ["categories", curOverrideCat, "terrain_override"],
        toFile: TERRAIN_FILE,
        toPath: ["categories", key, "terrain_override"],
        id: String(id),
      });
    } else {
      strokeEdits.push({
        kind: "addId",
        file: TERRAIN_FILE,
        listPath: ["categories", key, "terrain_override"],
        id: String(id),
      });
    }
    terrainOverlay.set(id, key);
    return true;
  }

  /// Whether a category already has a terrain_override block — from the backend
  /// `hasOverrideBlock` flag (authoritative, catches empty blocks too), or one
  /// created earlier in the committed queue. When absent we create it once
  /// (InsertStatement) before the first AddId.
  function terrainBlockExists(cat: string): boolean {
    const c = baseTerrain?.categories.find((x) => x.key === cat);
    if (c?.hasOverrideBlock) return true;
    for (const e of queue.serialize()) {
      if (
        e.kind === "insertStatement" &&
        e.file === TERRAIN_FILE &&
        e.blockPath.length === 2 &&
        e.blockPath[0] === "categories" &&
        e.blockPath[1] === cat
      ) {
        return true;
      }
    }
    return false;
  }

  /// Simple-terrain hover: the effective terrain of the province under the cursor
  /// (status footer) + the shared group hover highlight.
  function terrainHoverAt(clientX: number, clientY: number) {
    const id = provinceIdAt(clientX, clientY);
    const b = id !== NONE ? baseProv?.get(id) : undefined;
    if (!b || b.water || b.wasteland) {
      if (terrainHover !== null) terrainHover = null;
      setHover(NONE);
      return;
    }
    const key = terrainEffKey(id);
    const gi = key != null ? (terrainKeyToGroup.get(key) ?? NONE) : NONE;
    const name = gi !== NONE ? (modeData?.groups[gi]?.label ?? key ?? "—") : (key ?? "—");
    terrainHover = { id, terrain: key ?? "—", name, isOverride: terrainIsOverride(id) };
    setHover(gi);
  }

  /// Raw province id under a screen point, or NONE.
  function provinceIdAt(clientX: number, clientY: number): number {
    if (!provinceIds) return NONE;
    const rect = canvas.getBoundingClientRect();
    const x = Math.floor((clientX - rect.left - offsetX) / scale);
    const y = Math.floor((clientY - rect.top - offsetY) / scale);
    if (x < 0 || y < 0 || x >= mapW || y >= mapH) return NONE;
    return provinceIds[y * mapW + x];
  }

  /// Set-capital tool: a clicked province owned by the selected country becomes
  /// its capital (pending). The open CountryPanel writes the edit and disarms.
  function trySetCapital(clientX: number, clientY: number) {
    if (!selectedTag) return;
    const id = provinceIdAt(clientX, clientY);
    if (id === NONE) return;
    // Eligibility: the province must be owned by the selected country (pending-aware).
    if (effOf(id).owner !== selectedTag) return;
    capitalRequest = id;
    onArm(null);
  }

  /// Province-names "Pick on map" tool (Sprint 24): any clicked province id is
  /// handed to the province-names section that armed the pick.
  function tryPickProvName(clientX: number, clientY: number) {
    const id = provinceIdAt(clientX, clientY);
    if (id === NONE) return;
    provNamePick = id;
    onArm(null);
  }

  // --- Provinces mode: adjacencies (Sprint 25) ------------------------------

  /// Loads map/adjacencies.csv + the water id set for this session (once), then
  /// runs validation. Guarded by `seq` like the other mode loaders.
  async function loadAdjacencies(seq: number) {
    if (adjLoaded) {
      runAdjValidation();
      return;
    }
    try {
      const payload = await invoke<{ rows: AdjRow[]; waterIds: number[] }>(
        "get_adjacencies",
        { installPath, modPath },
      );
      if (seq !== renderSeq) return;
      adjBase = payload.rows;
      adjWater = new Set(payload.waterIds);
      adjLoaded = true;
      runAdjValidation();
    } catch (e) {
      console.error("get_adjacencies failed", e);
    }
  }

  /// Validates the effective (folded) adjacency list backend-side (sea-strait
  /// through-water / coastal endpoints / duplicate pairs). Fire-and-forget.
  async function runAdjValidation() {
    try {
      const issues = await invoke<AdjIssue[]>("validate_adjacencies", {
        installPath,
        modPath,
        rows: effectiveAdj,
      });
      adjIssues = issues;
    } catch (e) {
      console.error("validate_adjacencies failed", e);
    }
  }

  /// Pushes a full-list csvRewrite composite (one line-surgical rewrite; the
  /// backend re-emits unchanged rows byte-for-byte), then re-validates.
  function commitAdj(rows: AdjRowInput[], label: string, coalesceKey?: string) {
    queue.push({ label, edits: [rewriteEdit(rows)], coalesceKey });
    runAdjValidation();
  }

  /// Screen point → nearest adjacency line index (within tolerance), or null.
  function adjHitAt(clientX: number, clientY: number): number | null {
    // Hidden lines are not clickable — hit-testing follows what is drawn.
    if (!provinceIds || !showStraits) return null;
    const rect = canvas.getBoundingClientRect();
    return adjacencyAt(
      adjSegs,
      clientX - rect.left,
      clientY - rect.top,
      view,
      mapW,
      7,
    );
  }

  function updateAdjHover(clientX: number, clientY: number) {
    const i = adjHitAt(clientX, clientY);
    if (i !== hoverAdjIndex) hoverAdjIndex = i;
  }

  /// Selects the adjacency under the cursor (closing any province panel), or
  /// returns false so the caller falls through to province selection.
  function trySelectAdjacency(clientX: number, clientY: number): boolean {
    const i = adjHitAt(clientX, clientY);
    if (i == null) return false;
    if (selectedGroup !== NONE) select(NONE); // close the province panel
    selectedAdjIndex = i;
    return true;
  }

  /// Panel field change: replace the selected row (keeping its origin) and commit.
  function onAdjChange(next: AdjRow, coalesceKey: string) {
    if (selectedAdjIndex == null) return;
    const rows = effectiveAdj.map((r) => ({ ...r }));
    const prev = rows[selectedAdjIndex];
    if (!prev) return;
    rows[selectedAdjIndex] = { ...next, origin: prev.origin };
    commitAdj(rows, `Edit adjacency ${next.from}↔${next.to}`, `adj:${selectedAdjIndex}:${coalesceKey}`);
  }

  /// Delete the selected adjacency (row dropped from the list) and clear selection.
  function deleteAdj() {
    if (selectedAdjIndex == null) return;
    const gone = effectiveAdj[selectedAdjIndex];
    const rows = effectiveAdj.filter((_, i) => i !== selectedAdjIndex).map((r) => ({ ...r }));
    commitAdj(rows, `Delete adjacency ${gone?.from}↔${gone?.to}`);
    selectedAdjIndex = null;
  }

  /// "+ Add strait": first click captures one endpoint, second click appends a
  /// new row (type derived from water-ness; through auto-suggested as the water
  /// province nearest the endpoints' midpoint), opens the editor, and disarms.
  function tryAdjAddClick(clientX: number, clientY: number) {
    const id = provinceIdAt(clientX, clientY);
    if (id === NONE) return;
    if (adjAddFirst == null) {
      adjAddFirst = id;
      return;
    }
    if (id === adjAddFirst) return; // need two distinct provinces
    const from = adjAddFirst;
    const to = id;
    const a = centroids.get(from);
    const b = centroids.get(to);
    let through = -1;
    if (a && b) {
      through = suggestThrough(
        [a.x, a.y],
        [b.x, b.y],
        adjWater,
        (x, y) => (x >= 0 && y >= 0 && x < mapW && y < mapH ? provinceIds![y * mapW + x] : NONE),
      );
    }
    const kind = deriveType(adjWater.has(from), adjWater.has(to));
    const newRow: AdjRowInput = {
      from,
      to,
      kind,
      through,
      startX: -1,
      startY: -1,
      stopX: -1,
      stopY: -1,
      comment: "",
      origin: null,
    };
    const rows = effectiveAdj.map((r) => ({ ...r }));
    rows.push(newRow);
    adjAddFirst = null;
    onArm(null);
    commitAdj(rows, `Add ${kind} adjacency ${from}↔${to}`);
    selectedAdjIndex = rows.length - 1; // open the editor on the new row
  }

  /// Re-pick the From/To endpoint of the selected adjacency by clicking a province.
  function tryAdjEndpointPick(which: "from" | "to", clientX: number, clientY: number) {
    if (selectedAdjIndex == null) return;
    const id = provinceIdAt(clientX, clientY);
    if (id === NONE) return;
    const cur = effectiveAdj[selectedAdjIndex];
    if (!cur) return;
    onAdjChange({ ...cur, [which]: id }, `adj-${which}`);
    onArm(null);
  }

  /// Pick the `through` water province of the selected adjacency by map click.
  function tryAdjThroughPick(clientX: number, clientY: number) {
    if (selectedAdjIndex == null) return;
    const id = provinceIdAt(clientX, clientY);
    if (id === NONE) return;
    const cur = effectiveAdj[selectedAdjIndex];
    if (!cur) return;
    onAdjChange({ ...cur, through: id }, "adj-through");
    onArm(null);
  }

  // --- Create country (Sprint 4.1) ----------------------------------------

  /// Auto-derive an adjective from a country name: trailing-vowel → "+n"
  /// (Estonia → Estonian), else "+ian" (Newland → Newlandian). Deliberately
  /// simple — both name and adjective are editable in the panel after save.
  function deriveAdjective(name: string): string {
    const n = name.trim();
    if (!n) return "New";
    const last = n[n.length - 1].toLowerCase();
    return "aeiou".includes(last) ? `${n}n` : `${n}ian`;
  }

  /// Builds the create-country composite (backend scaffold), registers the new
  /// tag as a political group so hit-testing/repaint/panel see it before save,
  /// and auto-selects it (opens the CountryPanel in pending-scaffold mode).
  async function doCreateCountry(provinceId: number, name: string) {
    if (!modeData) return;
    const adjective = deriveAdjective(name);
    let scaffold: CountryScaffold;
    try {
      scaffold = await invoke<CountryScaffold>("prepare_country_scaffold", {
        installPath,
        modPath,
        capitalId: provinceId,
        name,
        adjective,
        // Exclude tags already claimed by pending (unsaved) creates so a
        // second country scaffolded before Save never collides with the
        // first — no save-first restriction (Sprint 4 limitation lifted).
        excludeTags: [...pendingCreatedTags],
      });
    } catch (e) {
      notify(String(e));
      return;
    }

    const color = scaffold.color as Rgb;
    queue.push({
      label: `Create country ${name}`,
      edits: scaffold.edits as TypedEdit[],
    });

    // Register the new country as a political group. The scaffold's capital
    // owner/controller/add_core edits are ordinary history/provinces edits that
    // fold through `edited` → applyPendingToMap, which recolors the capital with
    // this group's color; hover then highlights the new country (4.3).
    const newIndex = modeData.groups.length;
    modeData.groups.push({ key: scaffold.tag, label: name, color });
    tagToGroup.set(scaffold.tag, newIndex);
    countryScaffoldSeeds.set(scaffold.tag, {
      tag: scaffold.tag,
      name,
      adjective,
      color,
      capitalId: provinceId,
    });

    // Auto-select → CountryPanel opens; recompute the pending projection now
    // that the group is registered (paints the capital its new color).
    selectedGroup = newIndex;
    refreshPending();
  }

  function acceptCountryName(name: string) {
    const args = countryFlow.submitName(name || "New Country");
    if (args) doCreateCountry(args.provinceId, args.name);
    armedTool = null;
    countryFlow.reset();
  }
  function cancelCountryFlow() {
    armedTool = null;
    countryFlow.cancel();
    countryFlow.reset();
  }
  /// Create-flow map click: the clicked land province (owned or uncolonized)
  /// becomes the capital. Water/wasteland shakes the banner and stays armed.
  function tryCreateCountryClick(clientX: number, clientY: number): boolean {
    if (countryFlowState.phase !== "awaiting-click") return false;
    const id = provinceIdAt(clientX, clientY);
    if (id === NONE) return true; // clicked off-map; stay armed
    const b = baseProv?.get(id);
    if (!b || b.water || b.wasteland) {
      createShakeKey++; // invalid target: shake feedback, stay armed
      return true;
    }
    const rect = canvas.getBoundingClientRect();
    countryFlow.mapClicked(id, { x: clientX - rect.left, y: clientY - rect.top });
    return true;
  }

  // Undo of a create-country reverts its edits (capital repaints via
  // refreshPending) but the just-created country may still be selected — close
  // the panel when its tag is no longer pending in the queue.
  $effect(() => {
    queue.version;
    if (mode !== "political") return;
    const t = selectedTag;
    if (t && countryScaffoldSeeds.has(t) && !pendingCreatedTags.has(t)) {
      select(NONE);
    }
  });

  /// Delete a pending-created (unsaved) country (Sprint S2.1): drop its create
  /// composite from the queue. `pendingCreatedTags` then no longer holds the tag,
  /// so the effect above closes the panel; refreshPending repaints the capital
  /// back to uncolonized. The scaffold seed is dropped so nothing lingers.
  function removePendingCreatedCountry(t: string) {
    queue.removeWhere((c) => isCreateCompositeFor(c, t));
    countryScaffoldSeeds.delete(t);
  }

  // --- Create religion (Sprint 5.4) ---------------------------------------

  function slugify(name: string, fallback = "new_religion"): string {
    return (
      name
        .toLowerCase()
        .replace(/[^a-z0-9]+/g, "_")
        .replace(/^_+|_+$/g, "") || fallback
    );
  }
  function religionKeyExists(k: string): boolean {
    if (religionKeyToGroup.has(k)) return true;
    return modeData?.groups.some((g) => g.key === k) ?? false;
  }
  function uniqueReligionKey(name: string): string {
    const base = slugify(name);
    let k = base;
    let i = 2;
    while (religionKeyExists(k)) k = `${base}_${i++}`;
    return k;
  }
  function hsvToRgb(h: number, s: number, v: number): Rgb {
    const c = v * s;
    const x = c * (1 - Math.abs(((h / 60) % 2) - 1));
    const m = v - c;
    let r = 0,
      g = 0,
      b = 0;
    if (h < 60) [r, g, b] = [c, x, 0];
    else if (h < 120) [r, g, b] = [x, c, 0];
    else if (h < 180) [r, g, b] = [0, c, x];
    else if (h < 240) [r, g, b] = [0, x, c];
    else if (h < 300) [r, g, b] = [x, 0, c];
    else [r, g, b] = [c, 0, x];
    return [
      Math.round((r + m) * 255),
      Math.round((g + m) * 255),
      Math.round((b + m) * 255),
    ];
  }
  /// A color maximally distant (in RGB) from every existing religion color.
  function distinctReligionColor(): Rgb {
    const existing = modeData?.groups.map((g) => g.color) ?? [];
    let best: Rgb = [128, 128, 128];
    let bestD = -1;
    for (let h = 0; h < 360; h += 15) {
      for (const s of [0.65, 0.9]) {
        for (const v of [0.55, 0.85]) {
          const c = hsvToRgb(h, s, v);
          let d = Infinity;
          for (const e of existing) {
            const dd =
              (c[0] - e[0]) ** 2 + (c[1] - e[1]) ** 2 + (c[2] - e[2]) ** 2;
            if (dd < d) d = dd;
          }
          if (d > bestD) {
            bestD = d;
            best = c;
          }
        }
      }
    }
    return best;
  }

  /// Builds the create-religion composite (block scaffold + loc + province
  /// convert), registers the new religion pending-side, and auto-selects it.
  /// When `newGroup` is passed (S2.3), the religion is wrapped in a brand-new
  /// group block (inserted first so the religion insert composes into it) and the
  /// group gets its own loc entry.
  async function doCreateReligion(provinceId: number, name: string, newGroup?: GroupScaffold) {
    if (!modeData || !baseProv) return;
    // Derive the group + a sibling icon/source from the clicked province's
    // current (pending-aware) religion.
    const gi = effectiveGroup(provinceId);
    if (gi === NONE) return;
    const currentRel = modeData.groups[gi]?.key;
    if (!currentRel) return;
    let sourceFile: string;
    let groupKey: string;
    let groupName: string;
    let siblingIcon: number;
    try {
      const d = await invoke<ReligionDetails>("get_religion_details", {
        installPath,
        modPath,
        key: currentRel,
      });
      sourceFile = d.source_file;
      groupKey = d.group_key;
      groupName = d.group_name;
      siblingIcon = d.icon ?? 1;
    } catch (e) {
      error = String(e);
      return;
    }
    // A new group overrides the target group/file: everything lands in the new
    // group's file (the sibling group's file the scaffold was built from).
    if (newGroup) {
      sourceFile = newGroup.source_file;
      groupKey = newGroup.group_key;
      groupName = newGroup.group_name;
    }

    const key = uniqueReligionKey(name);
    const color = distinctReligionColor();
    // Authored at column 0; the writer re-indents into the group block.
    const block =
      `${key} = {\n` +
      `\tcolor = { ${color[0]} ${color[1]} ${color[2]} }\n` +
      `\ticon = ${siblingIcon}\n` +
      `\tcountry = {\n` +
      `\t\ttolerance_own = 2\n` +
      `\t\ttolerance_heretic = -1\n` +
      `\t\ttolerance_heathen = -2\n` +
      `\t}\n` +
      `\theretic = { }\n` +
      `}`;

    const b = baseProv.get(provinceId);
    const provFile = b?.file ?? `history/provinces/${provinceId}.txt`;
    const present = religionEff(provinceId).present;
    const provEdit: TypedEdit = present
      ? { kind: "setScalar", file: provFile, path: ["religion"], value: key, quoted: false }
      : { kind: "insertStatement", file: provFile, blockPath: [], statement: `religion = ${key}` };

    // A new group is created first (same-file edits compose in queue order), so
    // the religion insert below finds the group in the evolving buffer.
    const groupEdits: TypedEdit[] = newGroup
      ? [
          { kind: "insertStatement", file: sourceFile, blockPath: [], statement: newGroup.block },
          { kind: "locOverride", key: newGroup.group_key, value: newGroup.group_name },
        ]
      : [];

    queue.push({
      label: newGroup ? `Create religion ${name} in new group ${groupName}` : `Create religion ${name}`,
      edits: [
        ...groupEdits,
        { kind: "insertStatement", file: sourceFile, blockPath: [groupKey], statement: block },
        { kind: "locOverride", key, value: name },
        provEdit,
      ],
    });

    // Register the new religion pending-side so mode data + selection see it.
    const newIndex = modeData.groups.length;
    modeData.groups.push({ key, label: name, color });
    religionKeyToGroup.set(key, newIndex);
    createdSeed = {
      key,
      group_key: groupKey,
      group_name: groupName,
      localized_name: name,
      color,
      icon: siblingIcon,
      country_modifiers: [
        { key: "tolerance_own", value: "2" },
        { key: "tolerance_heretic", value: "-1" },
        { key: "tolerance_heathen", value: "-2" },
      ],
      province_modifiers: [],
      heretics: [],
      enable_date: null,
      features: [],
      raw_remainder: [],
      source_file: sourceFile,
      raw_block_text: "",
      country_count: 0,
      province_count: 1,
      sample_tags: [],
      sample_provinces: [provinceId],
    };

    // Select the new religion, then recompute the pending projection now that its
    // group is registered (the queued provEdit paints it its new color).
    selectedGroup = newIndex;
    refreshPending();
  }

  // --- Create-in-new-group (S2.3 / S2.4) ----------------------------------
  // A checkbox on the religion/culture create toolbar routes an accepted name
  // through NewGroupModal, which collects the group name (+ graphical culture for
  // cultures) and the sibling to copy defaults/pools from, then scaffolds a new
  // group to wrap the new religion/culture.
  let religionNewGroup = $state(false);
  let cultureNewGroup = $state(false);
  let newGroupModalOpen = $state(false);
  let newGroupKind = $state<"religion" | "culture">("religion");
  let newGroupSibling = $state("");
  let newGroupList = $state<{ key: string; name: string }[]>([]);
  let pendingNewGroupCreate: { provinceId: number; name: string } | null = null;

  async function openNewGroupFor(kind: "religion" | "culture", provinceId: number, name: string) {
    newGroupKind = kind;
    pendingNewGroupCreate = { provinceId, name };
    newGroupSibling = "";
    newGroupList = [];
    // Default the "copy from" sibling to the clicked province's current group.
    const gi = effectiveGroup(provinceId);
    const cur = gi !== NONE ? modeData?.groups[gi]?.key : undefined;
    try {
      if (kind === "religion") {
        newGroupList = await invoke<{ key: string; name: string }[]>("list_religion_groups", { installPath, modPath });
        if (cur) {
          const d = await invoke<ReligionDetails>("get_religion_details", { installPath, modPath, key: cur });
          newGroupSibling = d.group_key;
        }
      } else {
        newGroupList = await invoke<{ key: string; name: string }[]>("list_culture_groups", { installPath, modPath });
        if (cur) {
          const d = await invoke<CultureDetails>("get_culture_details", { installPath, modPath, key: cur });
          newGroupSibling = d.group_key;
        }
      }
    } catch (e) {
      error = String(e);
    }
    newGroupModalOpen = true;
  }

  async function confirmNewGroupCreate(res: NewGroupResult) {
    const pending = pendingNewGroupCreate;
    pendingNewGroupCreate = null;
    newGroupModalOpen = false;
    if (!pending) return;
    try {
      if (newGroupKind === "religion") {
        const scaffold = await invoke<GroupScaffold>("prepare_religion_group_scaffold", {
          installPath,
          modPath,
          siblingGroupKey: res.sibling,
          name: res.name,
          existingKeys: newGroupList.map((g) => g.key),
        });
        await doCreateReligion(pending.provinceId, pending.name, scaffold);
      } else {
        const scaffold = await invoke<GroupScaffold>("prepare_culture_group_scaffold", {
          installPath,
          modPath,
          siblingGroupKey: res.sibling,
          name: res.name,
          graphicalCulture: res.graphicalCulture ?? "",
          existingKeys: newGroupList.map((g) => g.key),
        });
        await doCreateCulture(pending.provinceId, pending.name, scaffold);
      }
    } catch (e) {
      error = String(e);
    }
  }
  function cancelNewGroupCreate() {
    pendingNewGroupCreate = null;
    newGroupModalOpen = false;
  }

  function acceptReligionName(name: string) {
    const args = religionFlow.submitName(name || "New Religion");
    armedTool = null;
    religionFlow.reset();
    if (!args) return;
    if (religionNewGroup) openNewGroupFor("religion", args.provinceId, args.name);
    else doCreateReligion(args.provinceId, args.name);
  }
  function cancelReligionFlow() {
    armedTool = null;
    religionFlow.cancel();
    religionFlow.reset();
  }
  /// Create-flow map click: the clicked land province (with a religion) seeds
  /// the new religion's group. Returns true if the click was consumed.
  function tryCreateReligionClick(clientX: number, clientY: number): boolean {
    if (religionFlowState.phase !== "awaiting-click") return false;
    const id = provinceIdAt(clientX, clientY);
    if (id === NONE) return true; // stay armed
    const b = baseProv?.get(id);
    if (!b || b.water || b.wasteland) return true; // land only; stay armed
    if (effectiveGroup(id) === NONE) return true; // needs a current religion (for the group)
    const rect = canvas.getBoundingClientRect();
    religionFlow.mapClicked(id, { x: clientX - rect.left, y: clientY - rect.top });
    return true;
  }

  // --- Create culture (Sprint 6.4) ----------------------------------------

  function cultureKeyExists(k: string): boolean {
    if (cultureKeyToGroup.has(k)) return true;
    return modeData?.groups.some((g) => g.key === k) ?? false;
  }
  function uniqueCultureKey(name: string): string {
    const base = slugify(name, "new_culture");
    let k = base;
    let i = 2;
    while (cultureKeyExists(k)) k = `${base}_${i++}`;
    return k;
  }

  /// Builds the create-culture composite (block scaffold with starter name pools
  /// copied from a sibling in the group + loc + province convert), registers the
  /// new culture pending-side, and auto-selects it.
  async function doCreateCulture(provinceId: number, name: string, newGroup?: GroupScaffold) {
    if (!modeData || !baseProv) return;
    const gi = effectiveGroup(provinceId);
    if (gi === NONE) return;
    const siblingKey = modeData.groups[gi]?.key;
    if (!siblingKey) return;

    let sourceFile: string;
    let groupKey: string;
    let groupName: string;
    // Starter pools: the clicked province's culture is a valid sibling. Copy its
    // own pools, falling back to the group pools (the game needs male/dynasty
    // names to generate rulers).
    let male: string[];
    let female: string[];
    let dynasty: string[];
    try {
      const d = await invoke<CultureDetails>("get_culture_details", {
        installPath,
        modPath,
        key: siblingKey,
      });
      sourceFile = d.source_file;
      groupKey = d.group_key;
      groupName = d.group_name;
      male = d.male_names.length > 0 ? d.male_names : d.group_male_names;
      female = d.female_names.length > 0 ? d.female_names : d.group_female_names;
      dynasty = d.dynasty_names.length > 0 ? d.dynasty_names : d.group_dynasty_names;
    } catch (e) {
      error = String(e);
      return;
    }
    // Guarantee non-empty male/dynasty pools so rulers generate.
    if (male.length === 0) male = ["Aldric", "Berin", "Cael", "Doran"];
    if (dynasty.length === 0) dynasty = ["Aldering", "Berreth"];
    // A new group (S2.4) overrides the target group/file; the group already
    // carries its own copied name pools, so the member only needs its own.
    if (newGroup) {
      sourceFile = newGroup.source_file;
      groupKey = newGroup.group_key;
      groupName = newGroup.group_name;
    }

    const key = uniqueCultureKey(name);
    const color = distinctReligionColor();
    // Authored at column 0; the writer re-indents into the group block.
    const block =
      `${key} = {\n` +
      `\tmale_names = { ${poolBlockValue(male)} }\n` +
      `\tfemale_names = { ${poolBlockValue(female)} }\n` +
      `\tdynasty_names = { ${poolBlockValue(dynasty)} }\n` +
      `}`;

    const b = baseProv.get(provinceId);
    const provFile = b?.file ?? `history/provinces/${provinceId}.txt`;
    const present = cultureEff(provinceId).present;
    const provEdit: TypedEdit = present
      ? { kind: "setScalar", file: provFile, path: ["culture"], value: key, quoted: false }
      : { kind: "insertStatement", file: provFile, blockPath: [], statement: `culture = ${key}` };

    const groupEdits: TypedEdit[] = newGroup
      ? [
          { kind: "insertStatement", file: sourceFile, blockPath: [], statement: newGroup.block },
          { kind: "locOverride", key: newGroup.group_key, value: newGroup.group_name },
        ]
      : [];

    queue.push({
      label: newGroup ? `Create culture ${name} in new group ${groupName}` : `Create culture ${name}`,
      edits: [
        ...groupEdits,
        { kind: "insertStatement", file: sourceFile, blockPath: [groupKey], statement: block },
        { kind: "locOverride", key, value: name },
        provEdit,
      ],
    });

    // Register the new culture pending-side so mode data + selection see it.
    const newIndex = modeData.groups.length;
    modeData.groups.push({ key, label: name, color });
    cultureKeyToGroup.set(key, newIndex);
    createdCultureSeed = {
      key,
      group_key: groupKey,
      group_name: groupName,
      localized_name: name,
      primary: null,
      male_names: male,
      female_names: female,
      dynasty_names: dynasty,
      male_names_present: true,
      female_names_present: true,
      dynasty_names_present: true,
      group_male_names: [],
      group_female_names: [],
      group_dynasty_names: [],
      group_male_names_present: false,
      group_female_names_present: false,
      group_dynasty_names_present: false,
      group_graphical_culture: null,
      group_second_graphical_culture: null,
      raw_remainder: [],
      source_file: sourceFile,
      raw_block_text: "",
      primary_count: 0,
      primary_tags: [],
      accepted_count: 0,
      accepted_tags: [],
      province_count: 1,
      sample_provinces: [provinceId],
    };

    selectedGroup = newIndex;
    refreshPending();
  }

  function acceptCultureName(name: string) {
    const args = cultureFlow.submitName(name || "New Culture");
    armedTool = null;
    cultureFlow.reset();
    if (!args) return;
    if (cultureNewGroup) openNewGroupFor("culture", args.provinceId, args.name);
    else doCreateCulture(args.provinceId, args.name);
  }
  function cancelCultureFlow() {
    armedTool = null;
    cultureFlow.cancel();
    cultureFlow.reset();
  }
  /// Create-flow map click: the clicked land province (with a culture) seeds the
  /// new culture's group. Returns true if the click was consumed.
  function tryCreateCultureClick(clientX: number, clientY: number): boolean {
    if (cultureFlowState.phase !== "awaiting-click") return false;
    const id = provinceIdAt(clientX, clientY);
    if (id === NONE) return true;
    const b = baseProv?.get(id);
    if (!b || b.water || b.wasteland) return true; // land only; stay armed
    if (effectiveGroup(id) === NONE) return true; // needs a current culture (for the group)
    const rect = canvas.getBoundingClientRect();
    cultureFlow.mapClicked(id, { x: clientX - rect.left, y: clientY - rect.top });
    return true;
  }

  /// Jump to a province in Provinces mode (religion panel usage link).
  let pendingSelectProvince: number | null = null;
  function openProvince(id: number, tab?: "overview" | "economy" | "military" | "monuments" | "history" | "advanced") {
    openView({ kind: "province", id, ...(tab ? { tab } : {}) }, "reuse");
  }

  // --- Trade Nodes interaction (Sprint 8) ----------------------------------

  function screenXY(clientX: number, clientY: number): [number, number] {
    const rect = canvas.getBoundingClientRect();
    return [clientX - rect.left, clientY - rect.top];
  }
  function sameRouteRef(a: RouteRef | null, b: RouteRef | null): boolean {
    return a === b || (!!a && !!b && a.from === b.from && a.index === b.index);
  }

  /// Effective control points (top-left) of the selected route, or null.
  function selectedRouteControl(): Xy[] | null {
    if (!selectedRoute || !tradeNetwork) return null;
    const n = tradeNetwork.nodes.find((x) => x.key === selectedRoute!.from);
    const r = n?.outgoing[selectedRoute!.index];
    return r ? r.control.map((p) => [...p] as Xy) : null;
  }

  /// Trade-mode hover precedence: marker → arrow → territory.
  function tradeHover(clientX: number, clientY: number) {
    if (!tradeNetwork) return;
    const [sx, sy] = screenXY(clientX, clientY);
    let nHoverNode: string | null = null;
    let nHoverRoute: RouteRef | null = null;
    let nGroup = NONE;
    const m = markerAt(tradeNetwork, centroids, sx, sy, view, 11);
    if (m) {
      nHoverNode = m;
      nGroup = tradeNodeKeyToGroup.get(m) ?? NONE;
    } else {
      const r = routeAt(tradeNetwork, sx, sy, view, 6);
      if (r) nHoverRoute = r;
      else nGroup = groupAt(clientX, clientY);
    }
    // Trade-detail tooltip: list the trade-modifier names of the hovered province
    // (only when the overlay is on and the province carries any).
    if (showTradeDetails) {
      const pid = provinceIdAt(clientX, clientY);
      const names = pid !== NONE ? (tradeDetailBase.get(pid)?.modifiers ?? []).map((mo) => mo.name) : [];
      tradeDetailTooltip = names.length > 0 ? { x: sx, y: sy, names } : null;
    } else if (tradeDetailTooltip) {
      tradeDetailTooltip = null;
    }
    if (nHoverNode === hoverNode && sameRouteRef(nHoverRoute, hoverRoute) && nGroup === hoverGroup) return;
    hoverNode = nHoverNode;
    hoverRoute = nHoverRoute;
    hoverGroup = nGroup;
    hovering = nGroup !== NONE || nHoverNode !== null || nHoverRoute !== null;
    updateHighlight();
    redraw();
  }

  function startHandleDrag(index: number) {
    draggingHandle = index;
    editControl = selectedRouteControl();
  }
  function dragHandle(clientX: number, clientY: number) {
    if (draggingHandle < 0 || !editControl) return;
    const [mx, my] = toMap(clientX, clientY);
    const next = editControl.slice();
    next[draggingHandle] = [mx, my];
    editControl = next;
    pushControlEdit();
  }
  /// Pushes the live (coalesced) control reshape for the selected route.
  function pushControlEdit() {
    if (!selectedRoute || !editControl || !tradeNetwork) return;
    const n = tradeNetwork.nodes.find((x) => x.key === selectedRoute!.from);
    const file = n?.source_file || TRADE_NODE_FILE;
    queue.push({
      label: `Reshape route ${selectedRoute.from}`,
      edits: [
        {
          kind: "setBlock",
          file,
          path: [selectedRoute.from, `outgoing#${selectedRoute.index}`, "control"],
          value: controlToFileString(editControl, tradeNetwork.map_height),
        },
      ],
      coalesceKey: `tnctrl:${selectedRoute.from}:${selectedRoute.index}`,
    });
  }
  /// Insert a handle where the user clicked the selected route's curve.
  function insertHandle(clientX: number, clientY: number) {
    const ctrl = selectedRouteControl();
    if (!ctrl || !tradeNetwork) return;
    const [mx, my] = toMap(clientX, clientY);
    const idx = insertIndexAt(ctrl, [mx, my], view, tradeNetwork.map_width);
    ctrl.splice(idx, 0, [mx, my]);
    editControl = ctrl;
    pushControlEdit();
  }
  function deleteLastHandle() {
    const ctrl = editControl ?? selectedRouteControl();
    if (!ctrl || ctrl.length <= 2) return;
    editControl = ctrl.slice(0, -1);
    pushControlEdit();
  }

  /// Trade-mode click: insert handle (on the selected curve) / select node /
  /// select route / select territory.
  function handleTradeClick(clientX: number, clientY: number) {
    if (!tradeNetwork) return;
    const [sx, sy] = screenXY(clientX, clientY);
    if (selectedRoute) {
      const ctrl = selectedRouteControl();
      if (ctrl && handleAt(ctrl, sx, sy, view, 9) < 0) {
        const r = routeAt(tradeNetwork, sx, sy, view, 6);
        if (r && sameRouteRef(r, selectedRoute)) {
          insertHandle(clientX, clientY);
          return;
        }
      }
    }
    const m = markerAt(tradeNetwork, centroids, sx, sy, view, 11);
    if (m) {
      const gi = tradeNodeKeyToGroup.get(m);
      if (gi !== undefined) {
        selectedRoute = null;
        select(gi);
      }
      return;
    }
    const r = routeAt(tradeNetwork, sx, sy, view, 6);
    if (r) {
      const gi = tradeNodeKeyToGroup.get(r.from);
      if (gi !== undefined) select(gi);
      selectedRoute = r;
      updateHighlight();
      redraw();
      return;
    }
    const g = groupAt(clientX, clientY);
    select(g);
    if (g === NONE) selectedRoute = null;
  }

  /// Set-location tool: a clicked member province becomes the node's location.
  function trySetNodeLocation(clientX: number, clientY: number) {
    if (!selectedNodeKey || !tradeNetwork) return;
    const id = provinceIdAt(clientX, clientY);
    if (id === NONE) return;
    const n = tradeNetwork.nodes.find((x) => x.key === selectedNodeKey);
    if (!n || !n.members.includes(id)) return;
    const file = n.source_file || TRADE_NODE_FILE;
    queue.push({
      label: `Set location of ${selectedNodeKey}`,
      edits: [{ kind: "setScalar", file, path: [selectedNodeKey, "location"], value: String(id), quoted: false }],
    });
    onArm(null);
  }

  /// Add-route tool: click the target node's marker → derive + insert a route.
  async function tryAddRoute(clientX: number, clientY: number) {
    if (!selectedNodeKey || !tradeNetwork) return;
    const [sx, sy] = screenXY(clientX, clientY);
    const target = markerAt(tradeNetwork, centroids, sx, sy, view, 13);
    if (!target || target === selectedNodeKey) return;
    const from = selectedNodeKey;
    const n = tradeNetwork.nodes.find((x) => x.key === from);
    const file = n?.source_file || TRADE_NODE_FILE;
    onArm(null);
    try {
      const d = await invoke<DerivedRoute>("derive_route_geometry", {
        installPath,
        modPath,
        fromNode: from,
        toNode: target,
      });
      const stmt = await invoke<string>("scaffold_trade_route", {
        target,
        path: d.path,
        control: d.control_file,
      });
      queue.push({
        label: `Add route ${from} → ${target}`,
        edits: [{ kind: "insertStatement", file, blockPath: [from], statement: stmt }],
      });
    } catch (e) {
      error = String(e);
    }
  }

  // --- Create trade node (Sprint 8.5) --------------------------------------
  function nodeKeyExists(k: string): boolean {
    return tradeNodeKeyToGroup.has(k) || (baseNetwork?.nodes.some((n) => n.key === k) ?? false);
  }
  function uniqueNodeKey(name: string): string {
    const base = slugify(name, "new_trade_node");
    let k = base;
    let i = 2;
    while (nodeKeyExists(k)) k = `${base}_${i++}`;
    return k;
  }
  async function doCreateNode(provinceId: number, name: string) {
    if (!modeData || !tradeNetwork) return;
    const key = uniqueNodeKey(name);
    const color = distinctReligionColor();
    let stmt: string;
    try {
      stmt = await invoke<string>("scaffold_trade_node", { key, location: provinceId, color });
    } catch (e) {
      error = String(e);
      return;
    }
    queue.push({
      label: `Create trade node ${name}`,
      edits: [
        { kind: "appendText", file: TRADE_NODE_FILE, text: stmt },
        { kind: "locOverride", key, value: name },
      ],
    });
    // Register pending-side so mode data + selection see it (scaffold has color).
    const newIndex = modeData.groups.length;
    modeData.groups.push({ key, label: name, color });
    tradeNodeKeyToGroup.set(key, newIndex);
    createdNodeKeys.add(key);
    baseColorPresent.add(key);
    selectedRoute = null;
    selectedGroup = newIndex;
    refreshPending();
  }
  function acceptNodeName(name: string) {
    const args = tradeFlow.submitName(name || "New Trade Node");
    if (args) doCreateNode(args.provinceId, args.name);
    armedTool = null;
    tradeFlow.reset();
  }
  function cancelNodeFlow() {
    armedTool = null;
    tradeFlow.cancel();
    tradeFlow.reset();
  }
  function tryCreateNodeClick(clientX: number, clientY: number): boolean {
    if (tradeFlowState.phase !== "awaiting-click") return false;
    const id = provinceIdAt(clientX, clientY);
    if (id === NONE) return true; // stay armed
    const rect = canvas.getBoundingClientRect();
    tradeFlow.mapClicked(id, { x: clientX - rect.left, y: clientY - rect.top });
    return true;
  }

  // --- Node panel action callbacks -----------------------------------------
  function selectNodeByKey(key: string) {
    const gi = tradeNodeKeyToGroup.get(key);
    if (gi === undefined) return;
    selectedRoute = null;
    select(gi);
  }
  function selectRouteRef(ref: RouteRef | null) {
    selectedRoute = ref;
    updateHighlight();
    redraw();
  }
  function onNodeDeleted() {
    selectedRoute = null;
    select(NONE);
  }
  function tradeJump(j: JumpTarget) {
    if (j.kind === "node") openView({ kind: "trade-node", key: j.id }, "reuse");
    else if (j.kind === "province") openProvince(j.id);
  }

  // --- Create area / region (Sprint 10) ------------------------------------
  function geoKeyExists(k: string): boolean {
    return (
      areaKeyToGroup.has(k) ||
      regionKeyToGroup.has(k) ||
      (baseGeo?.areas.some((a) => a.key === k) ?? false) ||
      (baseGeo?.regions.some((r) => r.key === k) ?? false)
    );
  }
  function uniqueGeoKey(name: string, suffix: string): string {
    const base = slugify(name, "new") + suffix;
    let k = base;
    let i = 2;
    while (geoKeyExists(k)) k = `${base}_${i++}`;
    return k;
  }

  function registerGeoGroup(key: string, name: string): number {
    const gi = modeData!.groups.length;
    const color = geoHashColor(key);
    modeData!.groups.push({ key, label: name, color });
    if (mode === "areas") areaKeyToGroup.set(key, gi);
    else regionKeyToGroup.set(key, gi);
    return gi;
  }

  async function doCreateArea(provinceId: number, name: string) {
    if (!modeData || !geoNetwork) return;
    const key = uniqueGeoKey(name, "_area");
    // Steal the starting province from its current area, if any.
    const from = geoMemberCache.get(provinceId) ?? areaMembershipIndex(geoNetwork).get(provinceId) ?? null;
    let stmt: string;
    try {
      stmt = await invoke<string>("scaffold_area_block", { key, provinces: [provinceId] });
    } catch (e) {
      error = String(e);
      return;
    }
    const edits: TypedEdit[] = [];
    if (from) {
      edits.push({ kind: "removeId", file: geoNetwork.area_file, listPath: [from], id: String(provinceId) });
    }
    edits.push({ kind: "appendText", file: geoNetwork.area_file, text: stmt });
    edits.push({ kind: "locOverride", key, value: name });
    queue.push({ label: `Create area ${name}`, edits });
    selectedGroup = registerGeoGroup(key, name);
    refreshPending();
  }
  function acceptAreaName(name: string) {
    const args = areaFlow.submitName(name || "New Area");
    if (args) doCreateArea(args.provinceId, args.name);
    armedTool = null;
    areaFlow.reset();
  }
  function cancelAreaFlow() {
    armedTool = null;
    areaFlow.cancel();
    areaFlow.reset();
  }
  function tryCreateAreaClick(clientX: number, clientY: number): boolean {
    if (areaFlowState.phase !== "awaiting-click") return false;
    const id = provinceIdAt(clientX, clientY);
    if (id === NONE) return true;
    const b = baseProv?.get(id);
    if (b && (b.water || b.wasteland)) return true; // stay armed on non-land
    const rect = canvas.getBoundingClientRect();
    areaFlow.mapClicked(id, { x: clientX - rect.left, y: clientY - rect.top });
    return true;
  }

  async function doCreateRegion(provinceId: number, name: string) {
    if (!modeData || !geoNetwork) return;
    // The clicked province's area becomes the region's first member.
    const areaKey = areaMembershipIndex(geoNetwork).get(provinceId) ?? null;
    if (!areaKey) {
      notify("That province has no area — assign it to an area first (Areas mode).");
      return;
    }
    const key = uniqueGeoKey(name, "_region");
    let stmt: string;
    try {
      stmt = await invoke<string>("scaffold_region_block", { key, firstArea: areaKey });
    } catch (e) {
      error = String(e);
      return;
    }
    const edits: TypedEdit[] = [];
    // Steal the area from its current region, if any.
    const fromRegion = geoNetwork.regions.find((r) => r.areas.includes(areaKey))?.key ?? null;
    if (fromRegion) {
      edits.push({ kind: "removeId", file: geoNetwork.region_file, listPath: [fromRegion, "areas"], id: areaKey });
    }
    edits.push({ kind: "appendText", file: geoNetwork.region_file, text: stmt });
    edits.push({ kind: "locOverride", key, value: name });
    queue.push({ label: `Create region ${name}`, edits });
    selectedGroup = registerGeoGroup(key, name);
    refreshPending();
  }
  function acceptRegionName(name: string) {
    const args = regionFlow.submitName(name || "New Region");
    if (args) doCreateRegion(args.provinceId, args.name);
    armedTool = null;
    regionFlow.reset();
  }
  function cancelRegionFlow() {
    armedTool = null;
    regionFlow.cancel();
    regionFlow.reset();
  }
  function tryCreateRegionClick(clientX: number, clientY: number): boolean {
    if (regionFlowState.phase !== "awaiting-click") return false;
    const id = provinceIdAt(clientX, clientY);
    if (id === NONE) return true;
    const b = baseProv?.get(id);
    if (b && (b.water || b.wasteland)) return true;
    const rect = canvas.getBoundingClientRect();
    regionFlow.mapClicked(id, { x: clientX - rect.left, y: clientY - rect.top });
    return true;
  }

  // --- Area/Region panel action callbacks ----------------------------------
  function selectGroupByKey(key: string) {
    const gi = mode === "areas" ? areaKeyToGroup.get(key) : regionKeyToGroup.get(key);
    if (gi === undefined) return;
    select(gi);
  }
  function onGeoDeleted() {
    select(NONE);
  }
  /// Jump from a region's member-area link to that area in Areas mode.
  function jumpToAreaMode(areaKey: string) {
    openView({ kind: "area", key: areaKey }, "reuse");
  }
  function geoJump(j: JumpTarget) {
    if (j.kind === "province") openProvince(j.id);
    else if (j.kind === "area") jumpToAreaMode(j.id);
  }

  /// Trade-node membership brush with steal semantics.
  function applyTradeNodeToolTo(id: number): boolean {
    const nodeKey = selectedNodeKey;
    if (!nodeKey || !tradeNetwork) return false;
    const n = tradeNetwork.nodes.find((x) => x.key === nodeKey);
    if (!n) return false;
    const toFile = n.source_file || TRADE_NODE_FILE;
    const cur = tnPaintOverride.has(id) ? tnPaintOverride.get(id)! : (tnMemberCache.get(id) ?? null);
    if (armedTool === "tn_add") {
      if (cur === nodeKey) return false;
      if (cur == null) {
        strokeEdits.push({ kind: "addId", file: toFile, listPath: [nodeKey, "members"], id: String(id) });
      } else {
        const fromNode = tradeNetwork.nodes.find((x) => x.key === cur);
        strokeEdits.push({
          kind: "listMove",
          fromFile: fromNode?.source_file || TRADE_NODE_FILE,
          fromPath: [cur, "members"],
          toFile,
          toPath: [nodeKey, "members"],
          id: String(id),
        });
      }
      tnPaintOverride.set(id, nodeKey);
      return true;
    }
    if (armedTool === "tn_remove") {
      if (cur !== nodeKey) return false;
      strokeEdits.push({ kind: "removeId", file: toFile, listPath: [nodeKey, "members"], id: String(id) });
      tnPaintOverride.set(id, null);
      return true;
    }
    return false;
  }

  /// Areas membership brush (province granularity) with steal semantics.
  function applyAreaToolTo(id: number): boolean {
    const areaKey = selectedAreaKey;
    if (!areaKey || !geoNetwork) return false;
    // Land only — water/wastelands have no area.
    const b = baseProv?.get(id);
    if (!b || b.water || b.wasteland) return false;
    const file = geoNetwork.area_file;
    const cur = geoPaintOverride.has(id) ? geoPaintOverride.get(id)! : (geoMemberCache.get(id) ?? null);
    if (armedTool === "area_add") {
      if (cur === areaKey) return false;
      if (cur == null) {
        strokeEdits.push({ kind: "addId", file, listPath: [areaKey], id: String(id) });
      } else {
        strokeEdits.push({
          kind: "listMove",
          fromFile: file,
          fromPath: [cur],
          toFile: file,
          toPath: [areaKey],
          id: String(id),
        });
      }
      geoPaintOverride.set(id, areaKey);
      return true;
    }
    if (armedTool === "area_remove") {
      if (cur !== areaKey) return false;
      strokeEdits.push({ kind: "removeId", file, listPath: [areaKey], id: String(id) });
      geoPaintOverride.set(id, null);
      return true;
    }
    return false;
  }

  /// Regions membership brush (area granularity) with steal semantics. Painting a
  /// province moves its whole area in/out of the selected region. The per-province
  /// paint override (all of the area's provinces) also dedupes repeat hits.
  function applyRegionToolTo(id: number): boolean {
    const regionKey = selectedRegionKey;
    if (!regionKey || !geoNetwork) return false;
    const b = baseProv?.get(id);
    if (!b || b.water || b.wasteland) return false;
    const net = geoNetwork;
    // The province's area (base + pending) — regions contain areas, not provinces.
    const areaKey = areaMembershipIndex(net).get(id) ?? null;
    if (!areaKey) return false;
    const area = net.areas.find((a) => a.key === areaKey);
    const provinces = area?.provinces ?? [id];
    const file = net.region_file;
    const cur = geoPaintOverride.has(id) ? geoPaintOverride.get(id)! : (geoMemberCache.get(id) ?? null);
    const setAreaOverride = (v: string | null) => {
      for (const p of provinces) geoPaintOverride.set(p, v);
    };
    if (armedTool === "region_add") {
      if (cur === regionKey) return false;
      if (cur == null) {
        strokeEdits.push({ kind: "addId", file, listPath: [regionKey, "areas"], id: areaKey });
      } else {
        strokeEdits.push({
          kind: "listMove",
          fromFile: file,
          fromPath: [cur, "areas"],
          toFile: file,
          toPath: [regionKey, "areas"],
          id: areaKey,
        });
      }
      setAreaOverride(regionKey);
      return true;
    }
    if (armedTool === "region_remove") {
      if (cur !== regionKey) return false;
      strokeEdits.push({ kind: "removeId", file, listPath: [regionKey, "areas"], id: areaKey });
      setAreaOverride(null);
      return true;
    }
    return false;
  }

  // --- Brush painting ------------------------------------------------------

  /// Applies the armed tool to province `id` if eligible, mutating `edited`,
  /// appending its edits to the stroke, and returning whether it changed.
  function applyToolTo(id: number): boolean {
    if (armedTool === "paint_religion" || armedTool === "unpaint_religion") {
      return applyReligionToolTo(id);
    }
    if (armedTool === "paint_culture" || armedTool === "unpaint_culture") {
      return applyCultureToolTo(id);
    }
    if (armedTool === "tn_add" || armedTool === "tn_remove") {
      return applyTradeNodeToolTo(id);
    }
    if (armedTool === "area_add" || armedTool === "area_remove") {
      return applyAreaToolTo(id);
    }
    if (armedTool === "region_add" || armedTool === "region_remove") {
      return applyRegionToolTo(id);
    }
    if (armedTool === "col_add" || armedTool === "col_remove") {
      return applyColonialToolTo(id);
    }
    if (armedTool === "paint_good") {
      return applyTradeGoodToolTo(id);
    }
    if (armedTool === "hre_add" || armedTool === "hre_remove") {
      return applyHreToolTo(id);
    }
    if (armedTool === "climate_paint") {
      return applyClimateToolTo(id);
    }
    if (armedTool === "paint_terrain") {
      return applyTerrainToolTo(id);
    }
    const tag = selectedTag;
    if (!tag || !baseProv) return false;
    const b = baseProv.get(id);
    if (!b) return false;
    const s = effOf(id);

    if (armedTool === "add_province") {
      // Skip water/wasteland and provinces already owned by the selected tag.
      if (b.water || b.wasteland || s.owner === tag) return false;
      const e = mutEff(id);
      if (e.ownerPresent) {
        strokeEdits.push({ kind: "setScalar", file: b.file, path: ["owner"], value: tag, quoted: false });
      } else {
        strokeEdits.push({ kind: "insertStatement", file: b.file, blockPath: [], statement: `owner = ${tag}` });
      }
      if (e.controllerPresent) {
        strokeEdits.push({ kind: "setScalar", file: b.file, path: ["controller"], value: tag, quoted: false });
      } else {
        strokeEdits.push({ kind: "insertStatement", file: b.file, blockPath: [], statement: `controller = ${tag}` });
      }
      if (!e.cores.has(tag)) {
        strokeEdits.push({ kind: "insertStatement", file: b.file, blockPath: [], statement: `add_core = ${tag}` });
      }
      e.owner = tag;
      e.ownerPresent = true;
      e.controller = tag;
      e.controllerPresent = true;
      e.cores.add(tag);
      return true;
    }

    if (armedTool === "remove_province") {
      // Only affects land currently owned by the selected country.
      if (s.owner !== tag) return false;
      const e = mutEff(id);
      if (e.ownerPresent) {
        strokeEdits.push({ kind: "removeStatement", file: b.file, blockPath: [], key: "owner" });
      }
      if (e.controllerPresent) {
        strokeEdits.push({ kind: "removeStatement", file: b.file, blockPath: [], key: "controller" });
      }
      if (e.cores.has(tag)) {
        strokeEdits.push({ kind: "removeStatement", file: b.file, blockPath: [], key: "add_core", value: tag });
      }
      e.owner = null;
      e.ownerPresent = false;
      e.controller = null;
      e.controllerPresent = false;
      e.cores.delete(tag);
      return true;
    }

    if (armedTool === "occupy") return applyOccupyTo(id);
    if (armedTool === "restore_control") return applyRestoreTo(id);
    return false;
  }

  /// Occupation brush (Sprint 13.3): paint `controller = <selectedTag>` onto a
  /// land province owned by an enemy in one of the selected country's active
  /// wars. Writes match the province panel's controller field (set-if-present,
  /// else insert). Own/neutral/water/wasteland provinces are skipped silently.
  function applyOccupyTo(id: number): boolean {
    const tag = selectedTag;
    if (!tag || !baseProv) return false;
    const b = baseProv.get(id);
    if (!b || b.water || b.wasteland) return false;
    const s = effOf(id);
    if (s.owner == null || !occupyEnemySet.has(s.owner)) return false;
    if (s.controller === tag) return false;
    const e = mutEff(id);
    if (e.controllerPresent) {
      strokeEdits.push({ kind: "setScalar", file: b.file, path: ["controller"], value: tag, quoted: false });
    } else {
      strokeEdits.push({ kind: "insertStatement", file: b.file, blockPath: [], statement: `controller = ${tag}` });
    }
    e.controller = tag;
    e.controllerPresent = true;
    return true;
  }

  /// Restore-control brush (Sprint 13.3): reset `controller` back to the owner on
  /// a province of either side of the selected country's active wars where the
  /// effective controller differs from the owner.
  function applyRestoreTo(id: number): boolean {
    const tag = selectedTag;
    if (!tag || !baseProv) return false;
    const b = baseProv.get(id);
    if (!b || b.water || b.wasteland) return false;
    const s = effOf(id);
    if (s.owner == null || !occupyRestoreSet.has(s.owner)) return false;
    if (s.controller == null || s.controller === s.owner) return false;
    const e = mutEff(id);
    const ownerTag = e.owner as string; // owner != null (checked via s.owner)
    if (e.controllerPresent) {
      strokeEdits.push({ kind: "setScalar", file: b.file, path: ["controller"], value: ownerTag, quoted: false });
    } else {
      strokeEdits.push({ kind: "insertStatement", file: b.file, blockPath: [], statement: `controller = ${ownerTag}` });
    }
    e.controller = ownerTag;
    e.controllerPresent = true;
    return true;
  }

  /// Religion brush: paint (`religion = key`, land only) or remove (delete the
  /// key → no religion) on province `id`. Mirrors the political tool shape.
  function applyReligionToolTo(id: number): boolean {
    const rk = selectedReligionKey;
    if (!rk || !baseProv) return false;
    const b = baseProv.get(id);
    if (!b) return false;
    const s = religionEff(id);

    if (armedTool === "paint_religion") {
      if (b.water || b.wasteland || s.religion === rk) return false;
      const e = religionMutEff(id);
      if (e.present) {
        strokeEdits.push({ kind: "setScalar", file: b.file, path: ["religion"], value: rk, quoted: false });
      } else {
        strokeEdits.push({ kind: "insertStatement", file: b.file, blockPath: [], statement: `religion = ${rk}` });
      }
      e.religion = rk;
      e.present = true;
      return true;
    }

    if (armedTool === "unpaint_religion") {
      if (s.religion !== rk) return false;
      const e = religionMutEff(id);
      if (e.present) {
        strokeEdits.push({ kind: "removeStatement", file: b.file, blockPath: [], key: "religion" });
      }
      e.religion = null;
      e.present = false;
      return true;
    }
    return false;
  }

  /// Culture brush: paint (`culture = key`, land only) or remove (delete the key
  /// → no culture) on province `id`. Mirrors the religion tool shape.
  function applyCultureToolTo(id: number): boolean {
    const ck = selectedCultureKey;
    if (!ck || !baseProv) return false;
    const b = baseProv.get(id);
    if (!b) return false;
    const s = cultureEff(id);

    if (armedTool === "paint_culture") {
      if (b.water || b.wasteland || s.culture === ck) return false;
      const e = cultureMutEff(id);
      if (e.present) {
        strokeEdits.push({ kind: "setScalar", file: b.file, path: ["culture"], value: ck, quoted: false });
      } else {
        strokeEdits.push({ kind: "insertStatement", file: b.file, blockPath: [], statement: `culture = ${ck}` });
      }
      e.culture = ck;
      e.present = true;
      return true;
    }

    if (armedTool === "unpaint_culture") {
      if (s.culture !== ck) return false;
      const e = cultureMutEff(id);
      if (e.present) {
        strokeEdits.push({ kind: "removeStatement", file: b.file, blockPath: [], key: "culture" });
      }
      e.culture = null;
      e.present = false;
      return true;
    }
    return false;
  }

  /// Paints one brush circle centered at map coords (cx, cy). Returns whether
  /// any province changed; the caller repaints once per pointer event.
  function paintAt(cx: number, cy: number): boolean {
    if (!provinceIds) return false;
    const hits = new Set<number>();
    collectCircle(provinceIds, mapW, mapH, cx, cy, brushSize, hits);
    let changed = false;
    for (const id of hits) {
      if (strokeAffected.has(id)) continue;
      strokeAffected.add(id);
      if (applyToolTo(id)) changed = true;
    }
    return changed;
  }

  // Airbrush (Sprint 9.1): the dev tools opt into a per-frame tick that accrues
  // development while the button is held. The add/remove/paint tools are
  // single-stamp and return null (inert), so the shared brush API is unchanged.
  let continuousStop: (() => void) | null = null;
  function toolContinuous(id: string | null): ContinuousTick | null {
    if (id === "dev_raise" || id === "dev_lower") return devTick;
    return null;
  }

  /// Whether the dev airbrush can act on a province (land only; uncolonized land
  /// carries dev too). Water/wasteland are ineligible.
  function isDevPaintable(id: number): boolean {
    const b = baseProv?.get(id);
    return !!b && !b.water && !b.wasteland;
  }

  /// One airbrush frame: accrue `dtMs` of development onto every eligible province
  /// under the brush, committing whole points via the mix (largest-remainder), and
  /// repaint the gradient + stat overlay when anything crossed an integer.
  function devTick(ids: Set<number>, dtMs: number) {
    const dir: DevDir = armedTool === "dev_lower" ? -1 : 1;
    const dt = dtMs / 1000;
    let changed = false;
    for (const id of ids) {
      if (!isDevPaintable(id)) continue;
      let a = devStroke.get(id);
      if (!a) {
        const base = devEff(id);
        a = newDevAccum([...base.vals], [...base.present]);
        devStroke.set(id, a);
      }
      if (tickDevAccum(a, dt, devMix, dir) > 0) changed = true;
    }
    if (changed) applyDevPendingToMap();
  }

  function startStroke(clientX: number, clientY: number) {
    painting = true;
    strokeAffected = new Set();
    strokeEdits = [];
    strokeLabel =
      armedTool === "add_province"
        ? `Add provinces to ${selectedTag}`
        : armedTool === "remove_province"
          ? `Remove provinces from ${selectedTag}`
          : armedTool === "occupy"
            ? `Occupy provinces as ${selectedTag}`
            : armedTool === "restore_control"
              ? `Restore control to owners`
          : armedTool === "paint_religion"
            ? `Paint ${selectedReligionKey}`
            : armedTool === "unpaint_religion"
              ? `Remove ${selectedReligionKey} from provinces`
              : armedTool === "paint_culture"
                ? `Paint ${selectedCultureKey}`
                : armedTool === "unpaint_culture"
                  ? `Remove ${selectedCultureKey} from provinces`
                  : armedTool === "tn_add"
                    ? `Add provinces to ${selectedNodeKey}`
                    : armedTool === "tn_remove"
                      ? `Remove provinces from ${selectedNodeKey}`
                      : armedTool === "area_add"
                        ? `Add provinces to ${selectedAreaKey}`
                        : armedTool === "area_remove"
                          ? `Remove provinces from ${selectedAreaKey}`
                          : armedTool === "region_add"
                            ? `Add areas to ${selectedRegionKey}`
                            : armedTool === "region_remove"
                              ? `Remove areas from ${selectedRegionKey}`
                              : armedTool === "climate_paint"
                                ? `Paint ${climateSelKey ?? (climateSelSlot === "zone" ? "Temperate" : "No winter")}`
                                : armedTool === "paint_terrain"
                                  ? `Paint terrain ${selectedTerrainKey === AUTO_KEY ? "Auto" : selectedTerrainKey}`
                                  : armedTool === "hre_add"
                                    ? "Add provinces to the HRE"
                                    : armedTool === "hre_remove"
                                      ? "Remove provinces from the HRE"
                                      : `Paint ${selectedGoodKey}`;
    previewSet = new Set();
    tnPaintOverride = new Map();
    geoPaintOverride = new Map();
    climatePaintOverride = new Map();
    climateInsertedThisStroke = new Set();
    terrainInsertedThisStroke = new Set();
    if (mode === "development") devStroke = new Map();
    const [cx, cy] = toMap(clientX, clientY);
    lastPaintX = cx;
    lastPaintY = cy;
    // Dev is airbrush-only: the continuous tick does all the work (no discrete
    // initial stamp), so accrual starts from the exact hold time.
    if (mode !== "development" && paintAt(cx, cy)) applyPendingToMap();

    const tick = toolContinuous(armedTool);
    if (tick && provinceIds) {
      continuousStop = runContinuous(() => {
        const s = new Set<number>();
        collectCircle(provinceIds!, mapW, mapH, lastPaintX, lastPaintY, brushSize, s);
        return s;
      }, tick);
    }
  }

  function continueStroke(clientX: number, clientY: number) {
    const [cx, cy] = toMap(clientX, clientY);
    // Interpolate so a fast drag leaves no gaps between brush stamps; repaint
    // once after all samples in this event are applied.
    let changed = false;
    for (const [sx, sy] of strokeSamples(lastPaintX, lastPaintY, cx, cy, brushSize)) {
      if (paintAt(sx, sy)) changed = true;
    }
    lastPaintX = cx;
    lastPaintY = cy;
    if (changed) applyPendingToMap();
  }

  function endStroke() {
    if (!painting) return;
    painting = false;
    if (continuousStop) {
      continuousStop();
      continuousStop = null;
    }
    // Dev airbrush: translate the per-province accumulators into one composite of
    // setScalar/insert edits (leftover fractions are already discarded), labeled
    // by direction + province count. One stroke = one undo unit.
    if (mode === "development") {
      const dir: DevDir = armedTool === "dev_lower" ? -1 : 1;
      const edits: TypedEdit[] = [];
      let n = 0;
      for (const [id, a] of devStroke) {
        const comps = finalizeDevAccum(a, dir);
        if (comps.length === 0) continue;
        const file = baseProv?.get(id)?.file ?? `history/provinces/${id}.txt`;
        for (const c of comps) {
          edits.push(
            c.present
              ? { kind: "setScalar", file, path: [c.key], value: String(c.value), quoted: false }
              : { kind: "insertStatement", file, blockPath: [], statement: `${c.key} = ${c.value}` },
          );
        }
        n++;
      }
      devStroke = new Map();
      if (edits.length > 0) {
        const verb = dir > 0 ? "Raise" : "Lower";
        pushStroke(`${verb} development (${n} province${n === 1 ? "" : "s"})`, edits);
      } else {
        // Nothing committed (e.g. lowering fully-floored land): repaint clean.
        applyDevPendingToMap();
      }
      strokeAffected = new Set();
      return;
    }
    if (strokeEdits.length > 0) {
      // One stroke = one composite = one undo unit. Pushing bumps queue.version,
      // which reruns refreshPending() (identical state, no flicker). At a later
      // date the political/religion/culture/goods statements are folded into a
      // dated block per province and the composite is date-tagged (Sprint 12.3).
      pushStroke(strokeLabel, strokeEdits);
    }
    strokeEdits = [];
    strokeAffected = new Set();
    // The stroke's edits are now in the queue; the effective network reflects
    // them, so drop the live paint overlay.
    tnPaintOverride = new Map();
    geoPaintOverride = new Map();
    climatePaintOverride = new Map();
  }

  /// Live preview: highlight the provinces the armed brush would affect.
  function updatePreview(clientX: number, clientY: number) {
    if (!provinceIds || !brushArmed || !paintTarget) return;
    const [cx, cy] = toMap(clientX, clientY);
    const hits = new Set<number>();
    collectCircle(provinceIds, mapW, mapH, cx, cy, brushSize, hits);
    const next = new Set<number>();
    for (const id of hits) {
      if (isEligible(id)) next.add(id);
    }
    previewSet = next;
    updateHighlight();
    redraw();
  }

  /// Whether the armed tool would act on province `id` (no mutation).
  function isEligible(id: number): boolean {
    if (armedTool === "tn_add" || armedTool === "tn_remove") {
      if (!selectedNodeKey) return false;
      const cur = tnPaintOverride.has(id) ? tnPaintOverride.get(id)! : (tnMemberCache.get(id) ?? null);
      return armedTool === "tn_add" ? cur !== selectedNodeKey : cur === selectedNodeKey;
    }
    if (armedTool === "dev_raise" || armedTool === "dev_lower") {
      if (!isDevPaintable(id)) return false;
      if (armedTool === "dev_raise") return true;
      // Lower is only actionable while some component is still above the floor.
      const c = effectiveDevComps(id);
      return c.present.some((pr, k) => pr && c.vals[k] > DEV_FLOOR);
    }
    if (armedTool === "area_add" || armedTool === "area_remove") {
      if (!selectedAreaKey) return false;
      const bb = baseProv?.get(id);
      if (!bb || bb.water || bb.wasteland) return false;
      const cur = geoPaintOverride.has(id) ? geoPaintOverride.get(id)! : (geoMemberCache.get(id) ?? null);
      return armedTool === "area_add" ? cur !== selectedAreaKey : cur === selectedAreaKey;
    }
    if (armedTool === "region_add" || armedTool === "region_remove") {
      if (!selectedRegionKey) return false;
      const bb = baseProv?.get(id);
      if (!bb || bb.water || bb.wasteland) return false;
      const cur = geoPaintOverride.has(id) ? geoPaintOverride.get(id)! : (geoMemberCache.get(id) ?? null);
      return armedTool === "region_add" ? cur !== selectedRegionKey : cur === selectedRegionKey;
    }
    if (armedTool === "col_add" || armedTool === "col_remove") {
      if (!selectedColonialKey) return false;
      const bb = baseProv?.get(id);
      if (!bb || bb.water || bb.wasteland) return false;
      const cur = geoPaintOverride.has(id) ? geoPaintOverride.get(id)! : (colonialMemberCache.get(id) ?? null);
      return armedTool === "col_add" ? cur !== selectedColonialKey : cur === selectedColonialKey;
    }
    if (armedTool === "climate_paint") {
      if (climateSelSlot === null || !climateModel) return false;
      const bb = baseProv?.get(id);
      if (!bb || bb.water || bb.wasteland) return false;
      const cur = climatePaintOverride.has(id)
        ? climatePaintOverride.get(id)!
        : climateKey(climateModel, climateSelSlot, id);
      return climateSelKey === null ? cur !== null : cur !== climateSelKey;
    }
    if (armedTool === "paint_terrain") {
      if (!selectedTerrainKey) return false;
      const bb = baseProv?.get(id);
      if (!bb || bb.water || bb.wasteland) return false;
      if (selectedTerrainKey === AUTO_KEY) return terrainIsOverride(id);
      // Eligible unless it is already an override to this exact category.
      return !(terrainIsOverride(id) && terrainEffKey(id) === selectedTerrainKey);
    }
    if (!baseProv) return false;
    const b = baseProv.get(id);
    if (!b) return false;
    if (armedTool === "paint_good") {
      const gk = selectedGoodKey;
      if (!gk) return false;
      return !b.water && !b.wasteland && tradeGoodEff(id).good !== gk;
    }
    if (armedTool === "paint_religion" || armedTool === "unpaint_religion") {
      const rk = selectedReligionKey;
      if (!rk) return false;
      const s = religionEff(id);
      if (armedTool === "paint_religion") return !b.water && !b.wasteland && s.religion !== rk;
      return s.religion === rk;
    }
    if (armedTool === "paint_culture" || armedTool === "unpaint_culture") {
      const ck = selectedCultureKey;
      if (!ck) return false;
      const s = cultureEff(id);
      if (armedTool === "paint_culture") return !b.water && !b.wasteland && s.culture !== ck;
      return s.culture === ck;
    }
    if (armedTool === "hre_add" || armedTool === "hre_remove") {
      if (b.water || b.wasteland) return false;
      return armedTool === "hre_add" ? !hreEffMember(id) : hreEffMember(id);
    }
    const tag = selectedTag;
    if (!tag) return false;
    const s = effOf(id);
    if (armedTool === "add_province") return !b.water && !b.wasteland && s.owner !== tag;
    if (armedTool === "remove_province") return s.owner === tag;
    if (armedTool === "occupy") {
      if (b.water || b.wasteland) return false;
      return s.owner != null && occupyEnemySet.has(s.owner) && s.controller !== tag;
    }
    if (armedTool === "restore_control") {
      if (b.water || b.wasteland) return false;
      return (
        s.owner != null &&
        occupyRestoreSet.has(s.owner) &&
        s.controller != null &&
        s.controller !== s.owner
      );
    }
    return false;
  }

  function clearPreview() {
    if (previewSet.size === 0) return;
    previewSet = new Set();
    updateHighlight();
    redraw();
  }

  // Rebuild pending projection whenever the committed queue changes
  // (undo/redo/save/other edits) or the selected date changes (Sprint 12.3: the
  // date gates which dated composites are visible). Reads both to track.
  $effect(() => {
    queue.version;
    selectedDate;
    if (!painting) refreshPending();
  });

  // Keep the route-editor handles synced to the selected route's effective
  // control points, except while a handle is mid-drag (which owns editControl).
  $effect(() => {
    const sr = selectedRoute;
    const net = tradeNetwork;
    if (draggingHandle >= 0) return;
    if (sr && net) {
      const n = net.nodes.find((x) => x.key === sr.from);
      const r = n?.outgoing[sr.index];
      editControl = r ? r.control.map((p) => [...p] as Xy) : null;
    } else {
      editControl = null;
    }
  });

  // Disarm brush tools and drop any preview when the selection/mode can no
  // longer host the armed tool.
  $effect(() => {
    const politicalCtx = mode === "political" && selectedTag !== null;
    // Create-country arms with NO selection; keep it alive in that context.
    const createCountryCtx = mode === "political" && selectedTag === null;
    const religionCtx = mode === "religion";
    const cultureCtx = mode === "culture";
    const tradeCtx = mode === "trade_nodes";
    const areasCtx = mode === "areas";
    const regionsCtx = mode === "regions";
    const colonialCtx = isColonialMode;
    const goodsCtx = mode === "trade_goods";
    const devCtx = mode === "development";
    const climateCtx = mode === "climate" || mode === "winter";
    const terrainCtx = mode === "simple_terrain";
    // Province Colors hosts its own pc_* tools (add/expand/dissolve), which
    // manage their own selection and must not be auto-disarmed here.
    const pcCtx = mode === "province_colors";
    if (!politicalCtx && !createCountryCtx && !religionCtx && !cultureCtx && !tradeCtx && !areasCtx && !regionsCtx && !colonialCtx && !goodsCtx && !devCtx && !climateCtx && !terrainCtx && !pcCtx) {
      if (armedTool !== null) armedTool = null;
      if (previewSet.size > 0) clearPreview();
      return;
    }
    // Climate/terrain paint tools need a selected entry.
    if (climateCtx && armedTool === "climate_paint" && !hasClimateSel) {
      armedTool = null;
      if (previewSet.size > 0) clearPreview();
    }
    if (terrainCtx && armedTool === "paint_terrain" && selectedTerrainKey === null) {
      armedTool = null;
      if (previewSet.size > 0) clearPreview();
    }
    // Dev tools need no selection, but a stale non-dev tool must not linger.
    if (devCtx && armedTool !== null && armedTool !== "dev_raise" && armedTool !== "dev_lower") {
      armedTool = null;
      if (previewSet.size > 0) clearPreview();
    }
    // Selecting a country ends the create tool; a stray non-create tool with no
    // selection (e.g. brush after the selection cleared) also disarms.
    if (politicalCtx && armedTool === "create_country") {
      armedTool = null;
      countryFlow.reset();
    }
    // Occupation brushes vanish when the selected country has no active war.
    if (
      politicalCtx &&
      (armedTool === "occupy" || armedTool === "restore_control") &&
      !hasActiveWarNow
    ) {
      armedTool = null;
      if (previewSet.size > 0) clearPreview();
    }
    if (createCountryCtx && armedTool !== null && armedTool !== "create_country") {
      armedTool = null;
      if (previewSet.size > 0) clearPreview();
    }
    if (goodsCtx) {
      if (armedTool === "paint_good" && selectedGoodKey === null) {
        armedTool = null;
        if (previewSet.size > 0) clearPreview();
      }
    }
    // Religion paint tools need a selection; the create tool needs none.
    if (religionCtx) {
      if (
        (armedTool === "paint_religion" || armedTool === "unpaint_religion") &&
        selectedReligionKey === null
      ) {
        armedTool = null;
        if (previewSet.size > 0) clearPreview();
      }
      if (armedTool === "create_religion" && selectedReligionKey !== null) {
        armedTool = null;
        religionFlow.reset();
      }
    }
    if (cultureCtx) {
      if (
        (armedTool === "paint_culture" || armedTool === "unpaint_culture") &&
        selectedCultureKey === null
      ) {
        armedTool = null;
        if (previewSet.size > 0) clearPreview();
      }
      if (armedTool === "create_culture" && selectedCultureKey !== null) {
        armedTool = null;
        cultureFlow.reset();
      }
    }
    if (tradeCtx) {
      const needSel =
        armedTool === "tn_add" ||
        armedTool === "tn_remove" ||
        armedTool === "tn_set_location" ||
        armedTool === "tn_add_route";
      if (needSel && selectedNodeKey === null) {
        armedTool = null;
        if (previewSet.size > 0) clearPreview();
      }
      if (armedTool === "tn_create" && selectedNodeKey !== null) {
        armedTool = null;
        tradeFlow.reset();
      }
    }
    if (areasCtx) {
      if ((armedTool === "area_add" || armedTool === "area_remove") && selectedAreaKey === null) {
        armedTool = null;
        if (previewSet.size > 0) clearPreview();
      }
      if (armedTool === "area_create" && selectedAreaKey !== null) {
        armedTool = null;
        areaFlow.reset();
      }
    }
    if (regionsCtx) {
      if ((armedTool === "region_add" || armedTool === "region_remove") && selectedRegionKey === null) {
        armedTool = null;
        if (previewSet.size > 0) clearPreview();
      }
      if (armedTool === "region_create" && selectedRegionKey !== null) {
        armedTool = null;
        regionFlow.reset();
      }
    }
    if (colonialCtx) {
      if ((armedTool === "col_add" || armedTool === "col_remove") && selectedColonialKey === null) {
        armedTool = null;
        if (previewSet.size > 0) clearPreview();
      }
      if (armedTool === "col_create" && selectedColonialKey !== null) {
        armedTool = null;
        colonialFlow.reset();
      }
    }
  });

  // Political tools. Occupy / Restore-control (Sprint 13.3) appear only when the
  // selected country has ≥1 war active at the selected date; otherwise the two
  // occupation brushes are omitted (the war editor lives in the Diplomacy tab).
  const toolButtons = $derived<ToolButton[]>([
    { id: "add_province", label: "Add", icon: "＋", tooltip: "Add province (brush) — assign to this country" },
    { id: "remove_province", label: "Remove", icon: "－", tooltip: "Remove province (brush) — make uncolonized" },
    { id: "set_capital", label: "Set Capital", icon: "★", tooltip: "Set capital — click an owned province" },
    { id: "hre_add", label: "+HRE", icon: "♛", tooltip: "Add to the Holy Roman Empire (brush) — paints hre = yes on land provinces (date-aware)" },
    { id: "hre_remove", label: "−HRE", icon: "♛", tooltip: "Remove from the Holy Roman Empire (brush) — paints hre = no on land provinces (date-aware)" },
    { id: "add_core_only", label: "Add Core", icon: "◆", tooltip: "coming soon" },
    ...(hasActiveWarNow
      ? ([
          { id: "occupy", label: "Occupy", icon: "▲", tooltip: "Occupy (brush) — set controller to this country on enemy land in its active wars" },
          { id: "restore_control", label: "Restore", icon: "▽", tooltip: "Restore control (brush) — reset controller to owner on occupied land of either side" },
        ] as ToolButton[])
      : []),
  ]);
  const DISABLED_TOOLS = new Set(["add_core_only", "change_controller"]);

  const createCountryTool: ToolButton[] = [
    { id: "create_country", label: "Create Country", icon: "＋", tooltip: "Create a new country — click its capital province" },
  ];

  const religionTools: ToolButton[] = [
    { id: "paint_religion", label: "Add", icon: "＋", tooltip: "Paint this religion onto land provinces (brush)" },
    { id: "unpaint_religion", label: "Remove", icon: "－", tooltip: "Remove this religion (brush) — province becomes no-religion" },
  ];
  const createReligionTool: ToolButton[] = [
    { id: "create_religion", label: "Create", icon: "＋", tooltip: "Create a new religion" },
  ];

  const cultureTools: ToolButton[] = [
    { id: "paint_culture", label: "Add", icon: "＋", tooltip: "Paint this culture onto land provinces (brush)" },
    { id: "unpaint_culture", label: "Remove", icon: "－", tooltip: "Remove this culture (brush) — province becomes no-culture" },
  ];
  const createCultureTool: ToolButton[] = [
    { id: "create_culture", label: "Create", icon: "＋", tooltip: "Create a new culture" },
  ];

  const tradeNodeTools: ToolButton[] = [
    { id: "tn_add", label: "Add", icon: "＋", tooltip: "Add provinces to this node (brush; steals from other nodes)" },
    { id: "tn_remove", label: "Remove", icon: "－", tooltip: "Remove provinces from this node (brush) — becomes node-less" },
    { id: "tn_set_location", label: "Set Location", icon: "◎", tooltip: "Set the collection province — click a member province" },
    { id: "tn_add_route", label: "Add Route", icon: "➜", tooltip: "Add an outgoing route — click the target node's marker" },
  ];
  const createNodeTool: ToolButton[] = [
    { id: "tn_create", label: "Create Node", icon: "＋", tooltip: "Create a new trade node — click its location province" },
  ];

  const areaTools: ToolButton[] = [
    { id: "area_add", label: "Add", icon: "＋", tooltip: "Add provinces to this area (brush; steals from other areas)" },
    { id: "area_remove", label: "Remove", icon: "－", tooltip: "Remove provinces from this area (brush) — becomes area-less" },
  ];
  const createAreaTool: ToolButton[] = [
    { id: "area_create", label: "Create Area", icon: "＋", tooltip: "Create a new area — click its starting province" },
  ];
  const regionTools: ToolButton[] = [
    { id: "region_add", label: "Add", icon: "＋", tooltip: "Add areas to this region (brush; paints whole areas, steals from other regions)" },
    { id: "region_remove", label: "Remove", icon: "－", tooltip: "Remove areas from this region (brush) — becomes region-less" },
  ];
  const createRegionTool: ToolButton[] = [
    { id: "region_create", label: "Create Region", icon: "＋", tooltip: "Create a new region — click a province in its first area" },
  ];

  const colonialTools: ToolButton[] = [
    { id: "col_add", label: "Add", icon: "＋", tooltip: "Add provinces to this entry (brush; steals from other entries)" },
    { id: "col_remove", label: "Remove", icon: "－", tooltip: "Remove provinces (brush) — becomes unassigned" },
  ];
  const createColonialTool = $derived<ToolButton[]>([
    {
      id: "col_create",
      label: mode === "trade_companies" ? "Create Company" : "Create Region",
      icon: "＋",
      tooltip:
        mode === "trade_companies"
          ? "Create a new trade company — click its starting province"
          : "Create a new colonial region — click its starting province",
    },
  ]);

  const goodTools: ToolButton[] = [
    { id: "paint_good", label: "Paint", icon: "＋", tooltip: "Paint this trade good onto land provinces (brush)" },
  ];

  const climateTools: ToolButton[] = [
    { id: "climate_paint", label: "Paint", icon: "＋", tooltip: "Paint the selected climate/winter entry onto land (brush; steals within the slot)" },
  ];

  const terrainTools: ToolButton[] = [
    { id: "paint_terrain", label: "Paint", icon: "＋", tooltip: "Paint the selected terrain override onto land (brush; steals between override lists)" },
  ];

  const devTools: ToolButton[] = [
    { id: "dev_raise", label: "Raise", icon: "▲", tooltip: "Raise development (airbrush) — hold & drag over land; ~2 dev/sec" },
    { id: "dev_lower", label: "Lower", icon: "▼", tooltip: "Lower development (airbrush) — floors at 1" },
  ];

  function onArm(id: string | null) {
    // Any (dis)arm resets the two-click "+ Add strait" sequence (the two clicks
    // keep armedTool === "adj_add" between them, so this never fires mid-add).
    adjAddFirst = null;
    // Province-colors tools own their selection; (dis)arming or switching among
    // them resets it and any open name prompt.
    if (PC_TOOLS.has(armedTool ?? "") && id !== armedTool) {
      clearPcSelection();
      pcDissolveCandidates = new Set();
      pcNamePrompt = null;
    }
    if (id && PC_TOOLS.has(id)) {
      armedTool = id;
      hovering = false;
      clearPreview();
      return;
    }
    if (id && DISABLED_TOOLS.has(id)) {
      // Placeholder tools are inert; never actually arm them.
      armedTool = null;
      return;
    }
    // The create tools drive their own map-click flow (createEntityFlow).
    if (id === "create_country") {
      armedTool = "create_country";
      countryFlow.reset();
      countryFlow.arm();
      hoverGroup = NONE;
      hovering = false;
      clearPreview();
      return;
    }
    if (id === "create_religion") {
      armedTool = "create_religion";
      religionFlow.reset();
      religionFlow.arm();
      hoverGroup = NONE;
      hovering = false;
      clearPreview();
      return;
    }
    if (id === "create_culture") {
      armedTool = "create_culture";
      cultureFlow.reset();
      cultureFlow.arm();
      hoverGroup = NONE;
      hovering = false;
      clearPreview();
      return;
    }
    if (id === "tn_create") {
      armedTool = "tn_create";
      tradeFlow.reset();
      tradeFlow.arm();
      hoverGroup = NONE;
      hoverNode = null;
      hoverRoute = null;
      hovering = false;
      clearPreview();
      return;
    }
    if (id === "area_create") {
      armedTool = "area_create";
      areaFlow.reset();
      areaFlow.arm();
      hoverGroup = NONE;
      hovering = false;
      clearPreview();
      return;
    }
    if (id === "region_create") {
      armedTool = "region_create";
      regionFlow.reset();
      regionFlow.arm();
      hoverGroup = NONE;
      hovering = false;
      clearPreview();
      return;
    }
    if (id === "col_create") {
      armedTool = "col_create";
      colonialFlow.reset();
      colonialFlow.arm();
      hoverGroup = NONE;
      hovering = false;
      clearPreview();
      return;
    }
    if (armedTool === "create_country" && id !== "create_country") {
      countryFlow.reset();
    }
    if (armedTool === "create_religion" && id !== "create_religion") {
      religionFlow.reset();
    }
    if (armedTool === "create_culture" && id !== "create_culture") {
      cultureFlow.reset();
    }
    if (armedTool === "tn_create" && id !== "tn_create") {
      tradeFlow.reset();
    }
    if (armedTool === "area_create" && id !== "area_create") {
      areaFlow.reset();
    }
    if (armedTool === "region_create" && id !== "region_create") {
      regionFlow.reset();
    }
    if (armedTool === "col_create" && id !== "col_create") {
      colonialFlow.reset();
    }
    armedTool = id;
    if (id) {
      // Drop any group hover so the brush preview is the only transient
      // highlight while armed. Arming a dev brush also closes the click-to-edit
      // panel (9.1b) and clears its single-province hover.
      hoverGroup = NONE;
      hovering = false;
      devHoverId = NONE;
      devSelectedId = null;
      ensureProvincePolitical();
      updateHighlight();
      redraw();
    } else {
      clearPreview();
      brushCursor = { ...brushCursor, on: false };
    }
  }

  function redraw() {
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    if (!bitmap) return;
    const dpr = window.devicePixelRatio || 1;
    ctx.setTransform(scale * dpr, 0, 0, scale * dpr, offsetX * dpr, offsetY * dpr);
    // Nearest-neighbor once zoomed in keeps province edges crisp.
    ctx.imageSmoothingEnabled = scale < 2;
    // Categorical modes draw the compositor's base (pristine + recolor
    // overrides); other modes draw the rendered bitmap directly.
    // Development is gradient but composites its pending recolor like categorical.
    const usesComp = categorical || mode === "development";
    // Province Colors draws its own editable canvas (saved bitmap + pending
    // pixel ops); categorical/dev draw the compositor; everything else the raw
    // rendered bitmap.
    const baseSource =
      mode === "province_colors" && pcEditCanvas
        ? pcEditCanvas
        : usesComp && compositor
          ? compositor.base
          : bitmap;
    ctx.drawImage(baseSource, 0, 0);
    if (
      mode === "province_colors" &&
      pcOverlay &&
      (pcSelectedId != null || pcTargets.size > 0)
    ) {
      ctx.drawImage(pcOverlay, 0, 0);
    }
    const tradeExtra =
      mode === "trade_nodes" && (showUnassigned || hoverRoute !== null || selectedRoute !== null);
    const devExtra =
      mode === "development" && (devHoverId !== NONE || devSelectedId != null || previewSet.size > 0);
    const climateExtra =
      (mode === "climate" || mode === "winter") &&
      (climateSelSlot !== null || climateHovering || previewSet.size > 0);
    if (
      usesComp &&
      compositor &&
      (hoverGroup !== NONE ||
        selectedGroup !== NONE ||
        previewSet.size > 0 ||
        tradeExtra ||
        devExtra ||
        climateExtra)
    ) {
      ctx.drawImage(compositor.overlay, 0, 0);
    }
    zoomPct = Math.round(scale * 100);
    // Publish the live transform to the trade-node overlay (new object → reactive).
    view = { scale, offsetX, offsetY };
  }

  function resize() {
    if (!container || !canvas) return;
    const dpr = window.devicePixelRatio || 1;
    canvas.width = container.clientWidth * dpr;
    canvas.height = container.clientHeight * dpr;
    canvas.style.width = `${container.clientWidth}px`;
    canvas.style.height = `${container.clientHeight}px`;
    cssW = container.clientWidth;
    cssH = container.clientHeight;
    redraw();
  }

  function fit() {
    if (!bitmap || !container) return;
    const s = Math.min(
      container.clientWidth / bitmap.width,
      container.clientHeight / bitmap.height,
    );
    scale = s;
    minScale = s * 0.5;
    offsetX = (container.clientWidth - bitmap.width * s) / 2;
    offsetY = (container.clientHeight - bitmap.height * s) / 2;
    redraw();
  }

  function onWheel(e: WheelEvent) {
    e.preventDefault();
    if (!bitmap) return;
    const rect = canvas.getBoundingClientRect();
    const mx = e.clientX - rect.left;
    const my = e.clientY - rect.top;
    const factor = Math.exp(-e.deltaY * 0.0015);
    const newScale = Math.min(MAX_SCALE, Math.max(minScale, scale * factor));
    const f = newScale / scale;
    offsetX = mx - (mx - offsetX) * f;
    offsetY = my - (my - offsetY) * f;
    scale = newScale;
    onCanvasWheelZoom();
    redraw();
  }

  function onPointerDown(e: PointerEvent) {
    // Trade-node route editor: grab a control-point handle instead of panning.
    if (mode === "trade_nodes" && e.button === 0 && !brushArmed && armedTool === null && selectedRoute) {
      const [sx, sy] = screenXY(e.clientX, e.clientY);
      const ctrl = selectedRouteControl();
      const hi = ctrl ? handleAt(ctrl, sx, sy, view, 9) : -1;
      if (hi >= 0) {
        canvas.setPointerCapture(e.pointerId);
        startHandleDrag(hi);
        return;
      }
    }
    // Left button + armed brush paints; other buttons still pan the map.
    if (brushArmed && e.button === 0 && paintTarget) {
      canvas.setPointerCapture(e.pointerId);
      clearPreview();
      startStroke(e.clientX, e.clientY);
      return;
    }
    // Province-colors pixel tools take the left button (drag = paint, click =
    // select); other buttons still pan.
    if (mode === "province_colors" && pcArmed && e.button === 0) {
      canvas.setPointerCapture(e.pointerId);
      pcOnDown(e.clientX, e.clientY);
      return;
    }
    dragging = true;
    lastX = downX = e.clientX;
    lastY = downY = e.clientY;
    moved = 0;
    canvas.setPointerCapture(e.pointerId);
  }

  function onPointerMove(e: PointerEvent) {
    if (draggingHandle >= 0) {
      dragHandle(e.clientX, e.clientY);
      return;
    }
    if (pcPointerActive) {
      pcOnMove(e.clientX, e.clientY);
      return;
    }
    if (painting) {
      continueStroke(e.clientX, e.clientY);
      setBrushCursor(e.clientX, e.clientY, true);
      return;
    }
    if (dragging) {
      offsetX += e.clientX - lastX;
      offsetY += e.clientY - lastY;
      lastX = e.clientX;
      lastY = e.clientY;
      moved = Math.max(
        moved,
        Math.abs(e.clientX - downX) + Math.abs(e.clientY - downY),
      );
      redraw();
    } else if (brushArmed) {
      setBrushCursor(e.clientX, e.clientY, true);
      updatePreview(e.clientX, e.clientY);
    } else if (mode === "province_colors" && pcArmed) {
      // New/Expand show the brush circle; Dissolve is a plain pointer.
      setBrushCursor(e.clientX, e.clientY, armedTool !== "pc_dissolve");
    } else if (mode === "trade_nodes") {
      tradeHover(e.clientX, e.clientY);
    } else if (mode === "development") {
      devHover(e.clientX, e.clientY);
    } else if (mode === "climate" || mode === "winter") {
      climateHover(e.clientX, e.clientY);
    } else if (mode === "simple_terrain") {
      terrainHoverAt(e.clientX, e.clientY);
    } else if (categorical) {
      if (mode === "political") politicalHoverId = provinceIdAt(e.clientX, e.clientY);
      if (mode === "provinces") updateAdjHover(e.clientX, e.clientY);
      setHover(groupAt(e.clientX, e.clientY));
    }
  }

  /// Universal "back out" (Esc and right-click), one step per press: disarm the
  /// active tool; else close the route editor / dev box; else clear the
  /// mode-specific list selection; else clear the map selection (closes the
  /// panel). Returns false when there was nothing to back out of.
  function backOut(): boolean {
    // Province-colors: first Esc clears the working selection, keeping the tool
    // armed; a second Esc disarms via the armedTool branch below.
    if (mode === "province_colors" && (pcSelectedId != null || pcTargets.size > 0)) {
      clearPcSelection();
      pcDissolveCandidates = new Set();
      return true;
    }
    if (armedTool) {
      onArm(null);
      return true;
    }
    if (mode === "trade_nodes" && selectedRoute) {
      selectedRoute = null;
      updateHighlight();
      redraw();
      return true;
    }
    if (mode === "development" && devSelectedId != null) {
      devSelectedId = null;
      updateHighlight();
      redraw();
      return true;
    }
    if (mode === "trade_goods" && selectedGoodKey !== null) {
      selectedGoodKey = null;
      select(NONE);
      return true;
    }
    if (
      (mode === "climate" || mode === "winter") &&
      (climateSelKey !== null || climateSelSlot !== null)
    ) {
      climateSelSlot = null;
      climateSelKey = null;
      updateClimateHighlight();
      redraw();
      return true;
    }
    if (mode === "simple_terrain" && selectedTerrainKey !== null) {
      selectedTerrainKey = null;
      select(NONE);
      return true;
    }
    if (mode === "provinces" && selectedAdjIndex != null) {
      selectedAdjIndex = null;
      return true;
    }
    if (selectedGroup !== NONE) {
      select(NONE);
      return true;
    }
    return false;
  }

  function onPointerUp(e: PointerEvent) {
    if (draggingHandle >= 0) {
      draggingHandle = -1;
      if (canvas.hasPointerCapture(e.pointerId)) {
        canvas.releasePointerCapture(e.pointerId);
      }
      return;
    }
    if (pcPointerActive) {
      pcOnUp(e.clientX, e.clientY, e.shiftKey);
      if (canvas.hasPointerCapture(e.pointerId)) {
        canvas.releasePointerCapture(e.pointerId);
      }
      return;
    }
    if (painting) {
      endStroke();
      if (canvas.hasPointerCapture(e.pointerId)) {
        canvas.releasePointerCapture(e.pointerId);
      }
      // Refresh the preview under the cursor for the next stroke.
      updatePreview(e.clientX, e.clientY);
      return;
    }
    const wasDrag = dragging && moved > 4;
    dragging = false;
    if (canvas.hasPointerCapture(e.pointerId)) {
      canvas.releasePointerCapture(e.pointerId);
    }
    // Right-click (no drag) = universal back-out; right-DRAG still pans.
    if (e.button === 2) {
      if (!wasDrag) backOut();
      return;
    }
    // A press that barely moved is a click: create flows, single-shot tools,
    // else select/clear.
    if (!wasDrag && categorical && !brushArmed) {
      if (armedTool === "create_country") {
        tryCreateCountryClick(e.clientX, e.clientY);
      } else if (armedTool === "create_religion") {
        tryCreateReligionClick(e.clientX, e.clientY);
      } else if (armedTool === "create_culture") {
        tryCreateCultureClick(e.clientX, e.clientY);
      } else if (armedTool === "tn_create") {
        tryCreateNodeClick(e.clientX, e.clientY);
      } else if (armedTool === "area_create") {
        tryCreateAreaClick(e.clientX, e.clientY);
      } else if (armedTool === "region_create") {
        tryCreateRegionClick(e.clientX, e.clientY);
      } else if (armedTool === "col_create") {
        tryCreateColonialClick(e.clientX, e.clientY);
      } else if (armedTool === "set_capital") {
        trySetCapital(e.clientX, e.clientY);
      } else if (armedTool === "pick_prov_name") {
        tryPickProvName(e.clientX, e.clientY);
      } else if (armedTool === "adj_add") {
        tryAdjAddClick(e.clientX, e.clientY);
      } else if (armedTool === "adj_pick_from") {
        tryAdjEndpointPick("from", e.clientX, e.clientY);
      } else if (armedTool === "adj_pick_to") {
        tryAdjEndpointPick("to", e.clientX, e.clientY);
      } else if (armedTool === "adj_pick_through") {
        tryAdjThroughPick(e.clientX, e.clientY);
      } else if (armedTool === "tn_set_location") {
        trySetNodeLocation(e.clientX, e.clientY);
      } else if (armedTool === "tn_add_route") {
        tryAddRoute(e.clientX, e.clientY);
      } else if (mode === "trade_nodes") {
        handleTradeClick(e.clientX, e.clientY);
      } else if (mode === "trade_goods") {
        // The list is the picker; a map click is an eyedropper (no eraser).
        selectGoodByGroup(groupAt(e.clientX, e.clientY));
      } else if (mode === "climate" || mode === "winter") {
        // Eyedropper: select the active-slot entry under the cursor.
        const id = provinceIdAt(e.clientX, e.clientY);
        const b = id !== NONE ? baseProv?.get(id) : undefined;
        if (b && !b.water && !b.wasteland) {
          const slot: ClimateSlot = mode === "climate" ? "zone" : "winter";
          const cur = climateCur(id, slot);
          climateSelSlot = slot;
          climateSelKey = cur;
          updateClimateHighlight();
          redraw();
        }
      } else if (mode === "simple_terrain") {
        // Eyedropper: select the effective terrain under the cursor.
        const id = provinceIdAt(e.clientX, e.clientY);
        const b = id !== NONE ? baseProv?.get(id) : undefined;
        if (b && !b.water && !b.wasteland) {
          const key = terrainEffKey(id);
          if (key) selectTerrain(key);
        }
      } else if (mode === "provinces" && trySelectAdjacency(e.clientX, e.clientY)) {
        // An adjacency line was clicked → its editor opened; skip province select.
      } else {
        selectedAdjIndex = null; // a province click closes any adjacency editor
        select(groupAt(e.clientX, e.clientY));
      }
    } else if (!wasDrag && mode === "development" && !brushArmed && armedTool === null) {
      // Click-to-edit (9.1b): deep-link to the province Economy content tab.
      const id = provinceIdAt(e.clientX, e.clientY);
      const b = id !== NONE ? baseProv?.get(id) : undefined;
      devSelectedId = b && !b.water && !b.wasteland ? id : null;
      if (devSelectedId != null) openProvince(devSelectedId, "economy");
      updateHighlight();
      redraw();
    }
  }

  function onPointerLeave() {
    if (painting) endStroke();
    brushCursor = { ...brushCursor, on: false };
    clearPreview();
    if (tradeDetailTooltip) tradeDetailTooltip = null;
    if (mode === "trade_nodes" && (hoverNode !== null || hoverRoute !== null)) {
      hoverNode = null;
      hoverRoute = null;
      updateHighlight();
      redraw();
    }
    if (mode === "development" && devHoverId !== NONE) {
      devHoverId = NONE;
      hovering = false;
      updateHighlight();
      redraw();
    }
    if ((mode === "climate" || mode === "winter") && climateHovering) {
      climateHoverKey = null;
      climateHovering = false;
      hovering = false;
      updateClimateHighlight();
      redraw();
    }
    if (mode === "simple_terrain" && terrainHover !== null) terrainHover = null;
    if (politicalHoverId !== NONE) politicalHoverId = NONE;
    if (!dragging && !brushArmed) setHover(NONE);
  }

  function onCanvasWheelZoom() {
    // Keep the brush circle sized correctly when zooming without moving.
    if ((brushArmed || pcBrushArmed) && brushCursor.on) {
      brushCursor = { ...brushCursor, d: Math.max(4, brushSize * scale) };
    }
  }

  // Guards against a slow render landing after a newer mode was selected.
  let renderSeq = 0;

  async function loadMap(first = false) {
    const seq = ++renderSeq;
    loading = true;
    error = "";
    try {
      const buf = await invoke<ArrayBuffer>("render_map_mode", {
        installPath,
        modPath,
        mode,
        date: selectedDate,
      });
      const next = await createImageBitmap(
        new Blob([buf], { type: "image/png" }),
      );
      if (seq !== renderSeq) {
        next.close();
        return;
      }
      bitmap?.close();
      bitmap = next;
      // Load this mode's selection/hover data + compositor before first draw.
      await loadModeInteraction(mode, seq);
      if (seq !== renderSeq) return;
      if (first) {
        resize();
        fit();
      } else {
        redraw();
      }
    } catch (e) {
      if (seq === renderSeq) error = String(e);
    }
    if (seq === renderSeq) loading = false;
  }

  function setMode(id: string) {
    if (id === mode) return;
    // Leaving Provinces mode clears its adjacency selection/hover.
    selectedAdjIndex = null;
    hoverAdjIndex = null;
    // Leaving Province Colors: disarm its tools and drop the working selection.
    if (mode === "province_colors" && id !== "province_colors") {
      if (PC_TOOLS.has(armedTool ?? "")) armedTool = null;
      clearPcSelection();
      pcDissolveCandidates = new Set();
      pcNamePrompt = null;
      // Free the ~33 MB color lookup + id buffer; rebuilt on re-entry.
      pcColorLut = null;
      pcIds = null;
    }
    mode = id;
    loadMap();
  }

  // --- Date selector context (Sprint 12.1/12.2) --------------------------------

  /// Loads the calendar/bookmark/defines context + the persisted selected date,
  /// resolving `selectedDate` before the first map render so the whole view
  /// derives at the right date. Best-effort: any failure falls back to vanilla.
  async function initDateContext() {
    try {
      const [bms, cal, defs] = await Promise.all([
        invoke<Bookmark[]>("get_bookmarks", { installPath, modPath }),
        invoke<CalendarLoc>("get_calendar_loc", { installPath, modPath }),
        invoke<DefinesDates>("get_defines_dates", { installPath, modPath }),
      ]);
      bookmarks = bms;
      baseCalendarMonths = cal.months;
      baseWorldYear = cal.worldYear;
      definesDates = defs;
      effectiveStart = effectiveStartDate(
        bms.map((b) => ({ date: b.date, isDefault: b.isDefault })),
      );
    } catch {
      effectiveStart = "1444.11.11";
    }
    let saved: string | null = null;
    try {
      saved = await invoke<string | null>("get_selected_date", { modPath });
    } catch {
      saved = null;
    }
    selectedDate = saved ?? effectiveStart;
    newDateValue = selectedDate;
  }

  /// Switches the view/edit date: persists it, invalidates date-derived caches,
  /// and re-derives the whole view (map image + mode data + panels via the date
  /// prop). The chip shows a busy spinner (`dateBusy`) while re-deriving.
  async function selectDate(d: string) {
    dateMenuOpen = false;
    newDateOpen = false;
    if (selectedDate === d) return;
    selectedDate = d;
    invoke("set_selected_date", { modPath, date: d }).catch(() => {});
    // Bulk political data is derived at the old date — drop it so it reloads.
    baseProv = null;
    dateBusy = true;
    try {
      await loadMap();
    } finally {
      dateBusy = false;
    }
  }

  /// Queues the "+ New start date…" composite (bookmark file + name/desc loc +
  /// defines override when out of range) then switches to the new date.
  async function createStartDate() {
    newDateError = "";
    const name = newDateName.trim();
    if (!name) {
      newDateError = "Enter a name for this start date.";
      return;
    }
    try {
      const s = await invoke<BookmarkScaffold>("scaffold_bookmark", {
        installPath,
        modPath,
        key: name,
        date: newDateValue,
      });
      const edits: TypedEdit[] = [
        { kind: "createFile", file: s.file, text: s.text },
        { kind: "locOverride", key: s.nameKey, value: name },
        {
          kind: "locOverride",
          key: s.descKey,
          value: `Start date added with EU Toolkit (${newDateValue}).`,
        },
      ];
      // Out of the current playable bounds → extend START/END_DATE so it loads.
      if (s.outOfRange) {
        if (compareDates(newDateValue, s.rangeStart) < 0) {
          edits.push({ kind: "setDefine", key: "START_DATE", value: newDateValue });
        }
        if (compareDates(newDateValue, s.rangeEnd) > 0) {
          edits.push({ kind: "setDefine", key: "END_DATE", value: newDateValue });
        }
      }
      queue.push({ label: `Add start date "${name}"`, edits });
      // Optimistically add the bookmark to the list so it appears immediately.
      bookmarks = [
        ...bookmarks,
        {
          file: s.file,
          nameKey: s.nameKey,
          name,
          descKey: s.descKey,
          desc: "",
          date: newDateValue,
          isDefault: false,
          center: null,
          countries: [],
        },
      ];
      if (definesDates && s.outOfRange) {
        definesDates = {
          startDate:
            compareDates(newDateValue, s.rangeStart) < 0 ? newDateValue : definesDates.startDate,
          endDate:
            compareDates(newDateValue, s.rangeEnd) > 0 ? newDateValue : definesDates.endDate,
        };
      }
      newDateName = "";
      await selectDate(newDateValue);
    } catch (e) {
      newDateError = String(e);
    }
  }

  // --- Playable-range validation (Sprint 12.4) --------------------------------

  /// A bookmark date outside the effective playable range won't load in game.
  function bookmarkRangeSide(date: string): "before" | "after" | null {
    if (!definesDates) return null;
    if (compareDates(date, definesDates.startDate) < 0) return "before";
    if (compareDates(date, definesDates.endDate) > 0) return "after";
    return null;
  }
  // Bookmarks whose date falls outside START_DATE..END_DATE (the warning set).
  let outOfRangeBookmarks = $derived(
    definesDates ? sortedBookmarks.filter((b) => bookmarkRangeSide(b.date) !== null) : [],
  );

  /// Reflects a START/END_DATE change in the optimistic `definesDates` so the
  /// range checks + the calendar editor fields update before save.
  function reflectRange(which: "START_DATE" | "END_DATE", value: string) {
    if (!definesDates) return;
    definesDates = {
      startDate: which === "START_DATE" ? value : definesDates.startDate,
      endDate: which === "END_DATE" ? value : definesDates.endDate,
    };
  }

  /// One-click "extend range" for an out-of-range bookmark: queues the setDefine
  /// that brings the offending bound out to the bookmark's date.
  function extendRangeToDate(date: string) {
    const side = bookmarkRangeSide(date);
    if (!side || !definesDates) return;
    const which = side === "before" ? "START_DATE" : "END_DATE";
    queue.push({
      label: `Extend playable ${side === "before" ? "start" : "end"} to ${date}`,
      edits: [{ kind: "setDefine", key: which, value: date }],
      coalesceKey: `define:${which}`,
    });
    reflectRange(which, date);
  }

  onMount(() => {
    void initializeWorkspace();
    invoke<MapMode[]>("list_map_modes")
      .then((m) => (modes = m))
      .catch(() => {});
    // A (re)opened session must re-read disk: the backend memoizes parsed game
    // data per session (cache.rs), and the mod may have changed externally
    // since the caches were built (e.g. git). Await it before the first
    // cache-consuming loads below.
    const cachesCleared = invoke("invalidate_caches").catch(() => {});
    // Sprint 28: load the scripted-name registry (link resolution in every 14.2
    // tree) and register the jump handler that opens the scripted browser.
    void cachesCleared.then(() => loadScriptedDefs(installPath, modPath));
    setScriptedJump((def: ScriptedDef) => {
      openView({kind:"scripted", focusKey: def.name});
    });
    // Gate File ▸ Fork from Steam… on whether this install has a Workshop folder.
    invoke<boolean>("is_steam_backed_install", { installPath })
      .then((v) => (steamBacked = v))
      .catch(() => (steamBacked = false));
    // Restore the persisted "Trade details" view toggle (default on).
    invoke<boolean | null>("get_view_toggle", { key: "trade_details" })
      .then((v) => {
        if (v !== null) {
          tradeDetailsLoaded = true;
          showTradeDetails = v;
        } else {
          tradeDetailsLoaded = true;
        }
      })
      .catch(() => (tradeDetailsLoaded = true));
    // Restore the persisted "Straits/Canals" view toggle (default on).
    invoke<boolean | null>("get_view_toggle", { key: "straits" })
      .then((v) => {
        if (v !== null) showStraits = v;
        straitsLoaded = true;
      })
      .catch(() => (straitsLoaded = true));
    // Resolve the selected date before the first render so the whole view derives
    // at the right date; fall back to an immediate render if context load fails.
    cachesCleared
      .then(() => initDateContext())
      .then(() => loadMap(true), () => loadMap(true));
    // Attached manually: Svelte's onwheel can be passive, and zoom needs preventDefault.
    canvas.addEventListener("wheel", onWheel, { passive: false });
    const ro = new ResizeObserver(() => resize());
    ro.observe(container);
    const onKey = (e: KeyboardEvent) => {
      // Route editor: Delete drops a control-point handle.
      if (
        (e.key === "Delete" || e.key === "Backspace") &&
        mode === "trade_nodes" &&
        selectedRoute
      ) {
        e.preventDefault();
        deleteLastHandle();
        return;
      }
      // Esc = universal back-out (same cascade as right-click): tool, route
      // editor, dev box, list selection, map selection — one step per press.
      if (e.key === "Escape" && !hasFocusedWorkspaceWindow() && backOut()) {
        e.preventDefault();
        return;
      }
      // Photoshop-style brush resize (no modifier) while a brush is armed.
      if (!e.ctrlKey && !e.metaKey && brushArmed) {
        if (e.key === "[") {
          e.preventDefault();
          brushSize = nudgeSize(brushSize, -1);
          if (brushCursor.on) updatePreview(brushCursor.x, brushCursor.y);
          return;
        }
        if (e.key === "]") {
          e.preventDefault();
          brushSize = nudgeSize(brushSize, 1);
          if (brushCursor.on) updatePreview(brushCursor.x, brushCursor.y);
          return;
        }
      }
      if (!e.ctrlKey && !e.metaKey) return;
      const key = e.key.toLowerCase();
      if (key === "f" && e.shiftKey) {
        // Ctrl+Shift+F: project-wide search (Sprint 30.3).
        e.preventDefault();
        openMenu = null;
        openView({kind:"search"});
      } else if (key === "s") {
        e.preventDefault();
        saveProject();
      } else if (key === "z" && !e.shiftKey) {
        e.preventDefault();
        queue.undo();
      } else if (key === "y" || (key === "z" && e.shiftKey)) {
        e.preventDefault();
        queue.redo();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => {
      canvas.removeEventListener("wheel", onWheel);
      window.removeEventListener("keydown", onKey);
      ro.disconnect();
      bitmap?.close();
      setScriptedJump(null);
    };
  });
</script>

<div class="map-screen" bind:this={container}>
  <canvas
    bind:this={canvas}
    class:grabbing={dragging}
    class:brushing={brushArmed}
    class:picking={armedTool === "set_capital" ||
      armedTool === "pick_prov_name" ||
      armedTool === "create_country" ||
      armedTool === "create_religion" ||
      armedTool === "create_culture"}
    class:hovering
    onpointerdown={onPointerDown}
    onpointermove={onPointerMove}
    onpointerup={onPointerUp}
    onpointercancel={onPointerUp}
    onpointerleave={onPointerLeave}
    oncontextmenu={(e) => e.preventDefault()}
  ></canvas>

  {#if mode === "trade_nodes"}
    <TradeNetworkOverlay
      network={tradeNetwork}
      {centroids}
      {view}
      cssWidth={cssW}
      cssHeight={cssH}
      selectedNode={selectedNodeKey}
      {hoverNode}
      {selectedRoute}
      {hoverRoute}
      {editControl}
      {showUnassigned}
    />
    <label class="unassigned-toggle">
      <input type="checkbox" bind:checked={showUnassigned} />
      Show unassigned
    </label>
  {/if}

  {#if mode === "provinces" && provinceIds && showStraits}
    <AdjacencyOverlay
      rows={effectiveAdj}
      segments={adjSegs}
      {view}
      cssWidth={cssW}
      cssHeight={cssH}
      {mapW}
      hoverIndex={hoverAdjIndex}
      selectedIndex={selectedAdjIndex}
    />
  {/if}

  {#if (mode === "trade_goods" || mode === "development" || (mode === "trade_nodes" && showTradeDetails)) && overlayAtlas && provinceIds}
    <IconOverlay
      {provinceIds}
      {mapW}
      {mapH}
      {view}
      cssWidth={cssW}
      cssHeight={cssH}
      items={overlayItems}
      atlas={overlayAtlas}
    />
  {/if}

  {#if tradeDetailTooltip && mode === "trade_nodes" && showTradeDetails}
    <div
      class="trade-detail-tip"
      style="left:{tradeDetailTooltip.x + 14}px; top:{tradeDetailTooltip.y + 14}px;"
    >
      {#each tradeDetailTooltip.names as n}
        <div class="tdt-row">{n}</div>
      {/each}
    </div>
  {/if}

  {#if mode === "development"}
    <DevMixSliders
      bind:mix={devMix}
      bind:locks={devLocks}
      atlas={overlayAtlas}
      atlasIndex={overlayAtlasIndex}
    />
  {/if}

  {#if openMenu}
    <button
      class="menu-backdrop"
      aria-label="Close menu"
      onclick={() => (openMenu = null)}
    ></button>
  {/if}

  <MapMenuBar>
    {#snippet children()}
    <div class="menu">
      <button
        class="menu-btn"
        class:open={openMenu === "file"}
        onclick={() => (openMenu = openMenu === "file" ? null : "file")}
      >
        File
      </button>
      {#if openMenu === "file"}
        <div class="dropdown">
          <button onclick={saveProject} disabled={!dirty || saving}>
            Save Project <span class="shortcut">Ctrl+S</span>
          </button>
          <button onclick={exportToGame} disabled={modPath === null}>
            Export to Game
          </button>
          <button
            onclick={exportAndLaunch}
            disabled={modPath === null}
            title="Register the mod in dlc_load.json and launch EU4 with it active"
          >
            Export &amp; Launch Game
          </button>
          <hr />
          <MenuFlyout
            label="New Project"
            items={[
              { label: "Start from base game", action: menuNewFromBase },
              { label: "Start from blank", action: menuNewFromBlank },
            ]}
          />
          <button onclick={menuOpenProject}>Open Project…</button>
          <button onclick={menuOpenBase} disabled={modPath === null}>
            Open Base Game
          </button>
          <button
            onclick={menuForkFromSteam}
            disabled={!steamBacked}
            title={steamBacked
              ? "Copy a subscribed Steam Workshop mod into an editable project"
              : "This installation isn't a Steam install with a Workshop folder"}
          >
            Fork from Steam…
          </button>
          <hr />
          <button onclick={menuHome}>Back to Launch Screen</button>
        </div>
      {/if}
    </div>
    <div class="menu">
      <button
        class="menu-btn"
        class:open={openMenu === "edit"}
        onclick={() => (openMenu = openMenu === "edit" ? null : "edit")}
      >
        Edit
      </button>
      {#if openMenu === "edit"}
        <div class="dropdown">
          <button onclick={doUndo} disabled={!queue.canUndo}>
            Undo{queue.undoLabel ? ` ${queue.undoLabel}` : ""}
            <span class="shortcut">Ctrl+Z</span>
          </button>
          <button onclick={doRedo} disabled={!queue.canRedo}>
            Redo{queue.redoLabel ? ` ${queue.redoLabel}` : ""}
            <span class="shortcut">Ctrl+Y</span>
          </button>
        </div>
      {/if}
    </div>
    <div class="menu">
      <button
        class="menu-btn"
        class:open={openMenu === "view"}
        onclick={() => (openMenu = openMenu === "view" ? null : "view")}
      >
        View
      </button>
      {#if openMenu === "view"}
        <div class="dropdown">
          <button
            onclick={() => {
              openMenu = null;
              fit();
            }}>Fit Map</button
          >
          <button onclick={zoomTo100}>Zoom 100%</button>
          <hr />
          <button
            class="toggle-item"
            title="Show the pending edit queue (undo units) with jump / revert actions"
            onclick={() => { openMenu = null; openView({kind:"edits"}); }}
          >
            <span class="check"></span>
            Edits{#if dirty}<span class="menu-badge">{queue.composites.length}</span>{/if}
          </button>
          <button
            title="Validate the whole project across every domain"
            onclick={openProblems}
          >
            Problems…{#if problemsHasRun && (problemsErrorCount || problemsWarningCount)}<span
                class="menu-badge"
                class:err={problemsErrorCount > 0}
                >{problemsErrorCount + problemsWarningCount}</span
              >{/if}
          </button>
          <button
            title="Search every script and localisation file in the project"
            onclick={() => { openMenu = null; openView({kind:"search"}); }}
          >
            Find in Project… <span class="shortcut">Ctrl+Shift+F</span>
          </button>
          <button
            title="Browse the whole mod vs the base game (added / shadowed / hidden files)"
            onclick={() => { openMenu = null; openView({kind:"project-changes"}); }}
            disabled={modPath === null}
          >
            Project Changes…
          </button>
          <hr />
          <button
            class="toggle-item"
            title="Show center-of-trade icons and trade-modifier badges in Trade Nodes mode"
            onclick={() => (showTradeDetails = !showTradeDetails)}
          >
            <span class="check">{showTradeDetails ? "✓" : ""}</span>
            Trade details
          </button>
          <button
            class="toggle-item"
            title="Draw strait, canal, and land adjacency links in Provinces mode"
            onclick={() => (showStraits = !showStraits)}
          >
            <span class="check">{showStraits ? "✓" : ""}</span>
            Straits/Canals
          </button>
        </div>
      {/if}
    </div>
    <div class="menu">
      <button
        class="menu-btn"
        class:open={openMenu === "tools"}
        onclick={() => (openMenu = openMenu === "tools" ? null : "tools")}
      >
        Tools
      </button>
      {#if openMenu === "tools"}
        <div class="dropdown">
          <button onclick={() => { openMenu = null; openView({kind:"decisions"}); }}>
            Decisions…
          </button>
          <button onclick={() => { openMenu = null; openView({kind:"events"}); }}>
            Events…
          </button>
          <button onclick={() => { openMenu = null; openView({kind:"missions"}); }}>
            Missions…
          </button>
          <button onclick={() => { openMenu = null; openView({kind:"government-names"}); }}>
            Government names…
          </button>
          <button onclick={() => { openMenu = null; openView({ kind: "estates" }, "reuse"); }}>
            Estates…
          </button>
          <button onclick={() => { openMenu = null; openView({kind:"rebels"}); }}>
            Rebels…
          </button>
          <button onclick={() => { openMenu = null; openView({kind:"technology"}); }}>
            Technology…
          </button>
          <button
            title="Generic idea groups (common/ideas with a category) — national TAG ideas are edited in the country panel"
            onclick={() => { openMenu = null; openView({kind:"mechanics", family:"idea_groups"}); }}
          >
            Ideas…
          </button>
          <button onclick={() => { openMenu = null; openView({kind:"mechanics"}); }}>
            Mechanics…
          </button>
          <button onclick={() => { openMenu = null; openView({kind:"empires"}); }}>
            Empires…
          </button>
          <button onclick={() => { openMenu = null; openView({kind:"color-pools"}); }}>
            Color Pools…
          </button>
          <button onclick={() => { openMenu = null; dynastiesOpen = true; }}>
            Dynasties…
          </button>
          <hr />
          <button onclick={() => { openMenu = null; openView({kind:"scripted"}); }}>
            Scripted Triggers/Effects…
          </button>
          <button onclick={() => { openMenu = null; openView({kind:"on-actions"}); }}>
            On Actions…
          </button>
          <button onclick={() => { openMenu = null; openView({kind:"localisation"}); }}>
            Localisation…
          </button>
          <button onclick={() => { openMenu = null; openView({kind:"defines"}); }}>
            Defines…
          </button>
        </div>
      {/if}
    </div>

    <div class="menu">
      <button class="menu-btn" onclick={() => { openMenu = null; openView({kind:"shortcuts"}); }}>Help</button>
    </div>

    {#if renaming}
      <input
        class="title title-edit"
        bind:this={renameInput}
        bind:value={renameValue}
        onblur={commitRename}
        onkeydown={renameKey}
        aria-label="Project name"
      />
    {:else if modPath}
      <button class="title title-btn" onclick={startRename} title="Rename project">
        {projectName ?? "Base Game"}{dirty ? " •" : ""}
      </button>
    {:else}
      <span class="title">Base Game{dirty ? " •" : ""}</span>
    {/if}
    <span class="path" title={modPath ?? installPath}>
      {modPath ?? installPath}
    </span>
    {#if saveMessage}
      <span class="saved">{saveMessage}</span>
    {/if}
    <span class="zoom">{zoomPct}%</span>
    {/snippet}
  </MapMenuBar>

  {#if modes.length > 0}
    <div class="modes">
      <h3>Map Modes</h3>
      <ul>
        {#each modes as m}
          <li>
            <button
              class="mode"
              class:active={m.id === mode}
              disabled={loading}
              onclick={() => setMode(m.id)}
            >
              <span class="mode-label">{m.label}</span>
              {#if m.viewOnly}<span class="view-only">View Only</span>{/if}
            </button>
          </li>
        {/each}
      </ul>
    </div>
  {/if}

  <!-- Edits panel (Sprint 30.1): left-docked, toggled from View ▸ Edits. -->

  <!-- Date selector chip (Sprint 12.1): top-right, map-anchored chrome (z-9). -->
  <div class="date-chip-wrap">
    <button
      class="date-chip"
      class:busy={dateBusy}
      title="Viewing the map at this date. Click to pick a start date."
      onclick={() => (dateMenuOpen = !dateMenuOpen)}
    >
      <span class="cal-glyph" aria-hidden="true">📅</span>
      <span class="date-text">{dateLabel}</span>
      {#if dateBusy}<span class="date-spinner" aria-label="Re-deriving"></span>{/if}
      <span class="caret" aria-hidden="true">▾</span>
    </button>

    {#if dateMenuOpen}
      <button
        class="date-backdrop"
        aria-label="Close date menu"
        onclick={() => {
          dateMenuOpen = false;
          calendarEditOpen = false;
        }}
      ></button>
      <div class="date-menu">
        {#if calendarEditOpen}
          <CalendarEditor
            {queue}
            months={calendarMonths}
            worldYear={worldYearTemplate}
            startDate={definesDates?.startDate ?? "1444.11.11"}
            endDate={definesDates?.endDate ?? "1821.1.2"}
            onrange={reflectRange}
            onback={() => (calendarEditOpen = false)}
          />
        {:else}
        <div class="date-menu-head">Start date</div>
        {#if outOfRangeBookmarks.length > 0}
          <div class="range-warn" role="status">
            <span class="rw-icon" aria-hidden="true">⚠</span>
            <span class="rw-text">
              {outOfRangeBookmarks.length} bookmark{outOfRangeBookmarks.length === 1
                ? ""
                : "s"} fall outside the playable range ({definesDates?.startDate} –
              {definesDates?.endDate}) and won't load in game.
            </span>
          </div>
        {/if}
        <ul class="bookmark-list">
          {#each sortedBookmarks as b (b.file + b.nameKey)}
            {@const side = bookmarkRangeSide(b.date)}
            <li>
              <div class="bookmark-line" class:oor={side !== null}>
                <button
                  class="bookmark-row"
                  class:current={selectedDate != null &&
                    compareDates(selectedDate, b.date) === 0}
                  onclick={() => selectDate(b.date)}
                >
                  <span class="check">
                    {selectedDate != null && compareDates(selectedDate, b.date) === 0
                      ? "✓"
                      : ""}
                  </span>
                  <span class="bm-name">{b.name || b.nameKey || "(unnamed)"}</span>
                  <span class="bm-date">
                    {formatDate(b.date, calendar)}
                  </span>
                </button>
                {#if side !== null}
                  <button
                    class="extend-btn"
                    title="Extend the playable range so this bookmark loads (queues a defines edit)."
                    onclick={() => extendRangeToDate(b.date)}
                  >
                    Extend range
                  </button>
                {/if}
              </div>
            </li>
          {/each}
          {#if sortedBookmarks.length === 0}
            <li class="empty">No bookmarks found</li>
          {/if}
        </ul>
        <hr />
        {#if !newDateOpen}
          <button
            class="new-date-btn"
            onclick={() => {
              newDateOpen = true;
              newDateError = "";
              newDateValue = selectedDate ?? effectiveStart;
            }}
          >
            + New start date…
          </button>
          <button class="new-date-btn" onclick={() => (calendarEditOpen = true)}>
            ⚙ Edit calendar…
          </button>
        {:else}
          <div class="new-date-form">
            <label class="nd-label" for="nd-name">Name</label>
            <input
              id="nd-name"
              class="nd-name"
              bind:value={newDateName}
              placeholder="e.g. My Campaign"
              onkeydown={(e) => e.key === "Enter" && createStartDate()}
            />
            <div class="nd-label">Date</div>
            <DatePicker bind:value={newDateValue} />
            {#if definesDates && (compareDates(newDateValue, definesDates.startDate) < 0 || compareDates(newDateValue, definesDates.endDate) > 0)}
              <div class="nd-hint">
                Outside the playable range ({definesDates.startDate} –
                {definesDates.endDate}); the bounds will be extended so this date
                is playable.
              </div>
            {/if}
            {#if newDateError}<div class="nd-error">{newDateError}</div>{/if}
            <div class="nd-actions">
              <button class="nd-ok" onclick={createStartDate}>Add</button>
              <button
                class="nd-cancel"
                onclick={() => {
                  newDateOpen = false;
                  newDateError = "";
                }}
              >
                Cancel
              </button>
            </div>
          </div>
        {/if}
        {/if}
      </div>
    {/if}
  </div>

  <!-- Static-file modes ignore the selected date (12.3 spec, view-side note). -->
  {#if showStaticNote}
    <div class="static-note" role="status">
      These files are not date-aware — showing the base state (the selected date
      is ignored here).
    </div>
  {/if}

  {#if labelName}
    <div class="country-label">
      {labelName}
      {#if occupiedByLabel}<span class="occupied-by">occupied by {occupiedByLabel}</span>{/if}
    </div>
  {/if}

  {#if selectedReligionKey && !workspaceWindows().some((w) => w.tabs.some((t) => t.view.kind === "religion" && t.view.key === selectedReligionKey))}
    {#key selectedReligionKey}
      <ReligionPanel
        {installPath}
        {modPath}
        {queue}
        religionKey={selectedReligionKey}
        seed={createdSeed && createdSeed.key === selectedReligionKey ? createdSeed : null}
        oncolor={onReligionColor}
        onjumpcountry={openCountryInPolitical}
        onjumpprovince={openProvince}
        onopenmechanics={(fam) => openView({kind:"mechanics", family:fam})}
        onclose={() => select(NONE)}
      />
    {/key}
  {/if}

  {#if selectedCultureKey && !workspaceWindows().some((w) => w.tabs.some((t) => t.view.kind === "culture" && t.view.key === selectedCultureKey))}
    {#key selectedCultureKey}
      <CulturePanel
        {installPath}
        {modPath}
        {queue}
        cultureKey={selectedCultureKey}
        seed={createdCultureSeed && createdCultureSeed.key === selectedCultureKey
          ? createdCultureSeed
          : null}
        oncolor={onCultureColor}
        onjumpcountry={openCountryInPolitical}
        onjumpprovince={openProvince}
        provNamePick={provNamePick}
        onarmprovnamepick={() => onArm("pick_prov_name")}
        onprovnamepickconsumed={() => (provNamePick = null)}
        onclose={() => select(NONE)}
      />
    {/key}
  {/if}

  {#if mode === "trade_nodes" && selectedNode && tradeNetwork && !workspaceWindows().some((w) => w.tabs.some((t) => t.view.kind === "trade-node" && t.view.key === selectedNodeKey))}
    {#key selectedNodeKey}
      <TradeNodePanel
        {installPath}
        {modPath}
        {queue}
        network={tradeNetwork}
        node={selectedNode}
        colorPresent={selectedNodeColorPresent}
        mapH={tradeNetwork.map_height}
        issues={tradeIssues}
        {selectedRoute}
        onclose={() => select(NONE)}
        onselectnode={selectNodeByKey}
        onselectroute={selectRouteRef}
        onsetlocation={() => onArm("tn_set_location")}
        onaddroute={() => onArm("tn_add_route")}
        ondeleted={onNodeDeleted}
        onjump={tradeJump}
        onopenmechanics={(fam, key) => openView({kind:"mechanics", family:fam, focusKey:key})}
      />
    {/key}
  {/if}

  {#if mode === "areas" && selectedArea && geoNetwork && !workspaceWindows().some((w) => w.tabs.some((t) => t.view.kind === "area" && t.view.key === selectedAreaKey))}
    {#key selectedAreaKey}
      <AreaPanel
        {queue}
        network={geoNetwork}
        area={selectedArea}
        issues={geoIssues}
        onclose={() => select(NONE)}
        onjump={geoJump}
        ondeleted={onGeoDeleted}
      />
    {/key}
  {/if}

  {#if mode === "regions" && selectedRegion && geoNetwork && !workspaceWindows().some((w) => w.tabs.some((t) => t.view.kind === "region" && t.view.key === selectedRegionKey))}
    {#key selectedRegionKey}
      <RegionPanel
        {installPath}
        {modPath}
        {queue}
        network={geoNetwork}
        region={selectedRegion}
        issues={geoIssues}
        onclose={() => select(NONE)}
        onjumparea={jumpToAreaMode}
        onjump={geoJump}
        ondeleted={onGeoDeleted}
      />
    {/key}
  {/if}

  {#if isColonialMode && selectedColonialEntry && colonialData && !workspaceWindows().some((w) => w.tabs.some((t) => t.view.kind === "colonial" && t.view.key === selectedColonialKey))}
    {#key selectedColonialKey}
      <ColonialPanel
        {installPath}
        {modPath}
        {queue}
        data={colonialData}
        entry={selectedColonialEntry}
        issues={selectedColonialIssues}
        onclose={() => select(NONE)}
        onjump={colonialJump}
        ondeleted={onColonialDeleted}
        onopenmechanics={(fam, key) => openView({kind:"mechanics", family:fam, focusKey:key})}
      />
    {/key}
  {/if}

  {#if selectedProvinceId != null}
    <!-- Reserve the bottom toolbar dock (province tools land in a later sprint). -->
    <BottomToolbar tools={[]}>
      {#snippet children()}
        <span class="tool-lead">Province #{selectedProvinceId}</span>
      {/snippet}
    </BottomToolbar>
  {/if}

  {#if armedTool === "pick_prov_name"}
    <PromptBanner
      message="Click a province to name it · Esc cancels"
      oncancel={() => onArm(null)}
    />
  {/if}

  <!-- Provinces mode: adjacency editor + add-strait tool (Sprint 25). -->
  {#if mode === "provinces" && selectedAdj && !workspaceWindows().some((w) => w.tabs.some((t) => t.view.kind === "adjacency" && t.view.index === selectedAdjIndex))}
    {#key selectedAdjIndex}
      <AdjacencyPanel
        row={selectedAdj}
        waterIds={adjWater}
        issues={selectedAdjIssues}
        armed={armedTool}
        onchange={onAdjChange}
        onpickendpoint={(which) => onArm(which === "from" ? "adj_pick_from" : "adj_pick_to")}
        onpickthrough={() => onArm("adj_pick_through")}
        ondelete={deleteAdj}
        onclose={() => (selectedAdjIndex = null)}
      />
    {/key}
  {/if}

  {#if mode === "provinces"}
    {#if armedTool === "adj_add"}
      <PromptBanner
        message={adjAddFirst == null
          ? "Click the FIRST province of the adjacency · Esc cancels"
          : "Click the SECOND province · Esc cancels"}
        oncancel={() => onArm(null)}
      />
    {/if}
    {#if armedTool === "adj_pick_from" || armedTool === "adj_pick_to"}
      <PromptBanner
        message="Click a province for this endpoint · Esc cancels"
        oncancel={() => onArm(null)}
      />
    {/if}
    {#if armedTool === "adj_pick_through"}
      <PromptBanner
        message="Click the water province a fleet can block · Esc cancels"
        oncancel={() => onArm(null)}
      />
    {/if}
    {#if selectedProvinceId == null}
      <BottomToolbar
        tools={[
          {
            id: "adj_add",
            label: "Add strait",
            icon: "+",
            tooltip: "Click two provinces to add a sea/land/canal/lake adjacency",
          },
        ]}
        bind:armed={armedTool}
        onarm={onArm}
      >
        {#snippet children()}
          <span class="tool-lead">Adjacencies · {effectiveAdj.length}</span>
        {/snippet}
      </BottomToolbar>
    {/if}
  {/if}

  {#if mode === "province_colors"}
    {#if armedTool === "pc_new"}
      <PromptBanner
        message="Drag over land to carve a new province · Esc cancels"
        oncancel={() => onArm(null)}
      />
    {/if}
    {#if armedTool === "pc_expand"}
      <PromptBanner
        message={pcSelectedId == null
          ? "Click the province to expand · Esc cancels"
          : `Drag to grow province ${pcSelectedId}; Shift-click a neighbour to absorb it whole · Esc`}
        oncancel={() => onArm(null)}
      />
    {/if}
    {#if armedTool === "pc_dissolve"}
      <PromptBanner
        message={pcSelectedId == null
          ? "Click the province to dissolve · Esc cancels"
          : `Click bordering provinces to divide province ${pcSelectedId} among them (${pcTargets.size} chosen) · Esc`}
        oncancel={() => onArm(null)}
      />
    {/if}
    <BottomToolbar
      tools={[
        { id: "pc_new", label: "New", icon: "＋", tooltip: "Carve a new province: drag over land to paint its pixels" },
        { id: "pc_expand", label: "Expand", icon: "⤢", tooltip: "Click a province, then drag to grow it over neighbours (Shift-click a neighbour to absorb it whole)" },
        { id: "pc_dissolve", label: "Dissolve", icon: "－", tooltip: "Click a province, then click bordering provinces to divide its pixels among them" },
      ]}
      bind:armed={armedTool}
      onarm={onArm}
    >
      {#snippet children()}
        <span class="tool-lead">Province Colors · edit the map bitmap</span>
      {/snippet}
      {#snippet extra()}
        {#if armedTool === "pc_new" || armedTool === "pc_expand"}
          <MapBrush bind:size={brushSize} />
        {/if}
        {#if armedTool === "pc_dissolve" && pcSelectedId != null}
          <button
            type="button"
            class="pc-confirm"
            disabled={pcTargets.size === 0}
            onclick={confirmDissolve}
          >
            Dissolve into {pcTargets.size}
          </button>
        {/if}
      {/snippet}
    </BottomToolbar>
  {/if}

  {#if mode === "province_colors" && pcNamePrompt}
    <InlineNamePrompt
      x={pcNamePrompt.x}
      y={pcNamePrompt.y}
      bind:value={pcNewName}
      label="Province"
      onaccept={acceptPcName}
      oncancel={cancelPcName}
    />
  {/if}

  {#if mode === "political" && selectedTag}
    {#if brushArmed}
      <PromptBanner
        message="Click and drag to paint. [ / ] resize · Esc cancels"
        oncancel={() => onArm(null)}
      />
    {/if}
    {#if armedTool === "set_capital"}
      <PromptBanner
        message={`Click a province owned by ${selectedTag} to make it the capital · Esc cancels`}
        oncancel={() => onArm(null)}
      />
    {/if}
    <BottomToolbar tools={toolButtons} bind:armed={armedTool} onarm={onArm}>
      {#snippet children()}
        <span class="tool-lead">Editing {selectedTag}</span>
      {/snippet}
      {#snippet extra()}
        {#if brushArmed}
          <MapBrush bind:size={brushSize} />
        {/if}
      {/snippet}
    </BottomToolbar>
  {/if}

  <!-- Political mode: create tool when nothing is selected (Sprint 4.1). -->
  {#if mode === "political" && !selectedTag}
    {#if countryFlowState.phase === "awaiting-click"}
      <PromptBanner
        message="Click on capital province · Esc cancels"
        oncancel={cancelCountryFlow}
        shakeKey={createShakeKey}
      />
    {/if}
    <BottomToolbar tools={createCountryTool} bind:armed={armedTool} onarm={onArm}>
      {#snippet children()}
        <span class="tool-lead">Countries</span>
      {/snippet}
    </BottomToolbar>
  {/if}

  {#if countryFlowState.phase === "naming" && countryFlowState.position}
    <InlineNamePrompt
      x={countryFlowState.position.x}
      y={countryFlowState.position.y}
      value={countryFlowState.name ?? "New Country"}
      label="Country"
      onaccept={acceptCountryName}
      oncancel={cancelCountryFlow}
    />
  {/if}

  <!-- Religion mode: paint/remove tools when a religion is selected. -->
  {#if mode === "religion" && selectedReligionKey}
    {#if brushArmed}
      <PromptBanner
        message="Click and drag to paint. [ / ] resize · Esc cancels"
        oncancel={() => onArm(null)}
      />
    {/if}
    <BottomToolbar tools={religionTools} bind:armed={armedTool} onarm={onArm}>
      {#snippet children()}
        <span class="tool-lead">Editing {selectedReligionKey}</span>
      {/snippet}
      {#snippet extra()}
        {#if brushArmed}
          <MapBrush bind:size={brushSize} />
        {/if}
      {/snippet}
    </BottomToolbar>
  {/if}

  <!-- Religion mode: create tool when nothing is selected. -->
  {#if mode === "religion" && !selectedReligionKey}
    {#if religionFlowState.phase === "awaiting-click"}
      <PromptBanner
        message="Click the province where the religion starts · Esc cancels"
        oncancel={cancelReligionFlow}
      />
    {/if}
    <BottomToolbar tools={createReligionTool} bind:armed={armedTool} onarm={onArm}>
      {#snippet children()}
        <span class="tool-lead">Religions</span>
        <label class="ng-check" title="Wrap the new religion in a brand-new religion group">
          <input type="checkbox" bind:checked={religionNewGroup} /> New group
        </label>
      {/snippet}
    </BottomToolbar>
  {/if}

  {#if religionFlowState.phase === "naming" && religionFlowState.position}
    <InlineNamePrompt
      x={religionFlowState.position.x}
      y={religionFlowState.position.y}
      value={religionFlowState.name ?? "New Religion"}
      label="Religion"
      onaccept={acceptReligionName}
      oncancel={cancelReligionFlow}
    />
  {/if}

  <!-- Culture mode: paint/remove tools when a culture is selected. -->
  {#if mode === "culture" && selectedCultureKey}
    {#if brushArmed}
      <PromptBanner
        message="Click and drag to paint. [ / ] resize · Esc cancels"
        oncancel={() => onArm(null)}
      />
    {/if}
    <BottomToolbar tools={cultureTools} bind:armed={armedTool} onarm={onArm}>
      {#snippet children()}
        <span class="tool-lead">Editing {selectedCultureKey}</span>
      {/snippet}
      {#snippet extra()}
        {#if brushArmed}
          <MapBrush bind:size={brushSize} />
        {/if}
      {/snippet}
    </BottomToolbar>
  {/if}

  <!-- Culture mode: create tool when nothing is selected. -->
  {#if mode === "culture" && !selectedCultureKey}
    {#if cultureFlowState.phase === "awaiting-click"}
      <PromptBanner
        message="Click the province where the culture starts · Esc cancels"
        oncancel={cancelCultureFlow}
      />
    {/if}
    <BottomToolbar tools={createCultureTool} bind:armed={armedTool} onarm={onArm}>
      {#snippet children()}
        <span class="tool-lead">Cultures</span>
        <label class="ng-check" title="Wrap the new culture in a brand-new culture group">
          <input type="checkbox" bind:checked={cultureNewGroup} /> New group
        </label>
      {/snippet}
    </BottomToolbar>
  {/if}

  {#if cultureFlowState.phase === "naming" && cultureFlowState.position}
    <InlineNamePrompt
      x={cultureFlowState.position.x}
      y={cultureFlowState.position.y}
      value={cultureFlowState.name ?? "New Culture"}
      label="Culture"
      onaccept={acceptCultureName}
      oncancel={cancelCultureFlow}
    />
  {/if}

  <!-- Create-in-new-group modal (S2.3 / S2.4), shared by both create flows. -->
  <NewGroupModal
    bind:open={newGroupModalOpen}
    kind={newGroupKind}
    {installPath}
    {modPath}
    groups={newGroupList}
    defaultSibling={newGroupSibling}
    onconfirm={confirmNewGroupCreate}
    oncancel={cancelNewGroupCreate}
  />

  <!-- Trade Goods: list-driven picker/editor (7.1) + paint toolbar (7.2). -->
  {#if mode === "trade_goods"}
    <TradeGoodsList
      {installPath}
      {modPath}
      {queue}
      goods={allGoods}
      counts={goodCounts}
      selectedKey={selectedGoodKey}
      atlas={overlayAtlas}
      atlasIndex={overlayAtlasIndex}
      onselect={selectGood}
      oncolor={onTradeGoodColor}
      oncreate={doCreateTradeGood}
    />
    {#if selectedGoodKey}
      {#if brushArmed}
        <PromptBanner
          message="Click and drag to paint. [ / ] resize · Esc cancels"
          oncancel={() => onArm(null)}
        />
      {/if}
      <BottomToolbar tools={goodTools} bind:armed={armedTool} onarm={onArm}>
        {#snippet children()}
          <span class="tool-lead">
            Painting {selectedGoodKey === UNKNOWN_KEY ? "No trade good" : selectedGoodKey}
          </span>
        {/snippet}
        {#snippet extra()}
          {#if brushArmed}
            <MapBrush bind:size={brushSize} />
          {/if}
        {/snippet}
      </BottomToolbar>
    {/if}
  {/if}

  <!-- Climate / Winter: two-slot selector + paint brush (Sprint 11.1). -->
  {#if (mode === "climate" || mode === "winter") && climateModel}
    {#if !workspaceWindows().some((w) => w.tabs.some((t) => t.view.kind === "climate"))}<ClimatePanel
      model={climateModel}
      counts={climateCountMap}
      selSlot={climateSelSlot}
      selKey={climateSelKey}
      mode={mode === "winter" ? "winter" : "climate"}
      bind:showWinterTint
      onselect={selectClimateEntry}
    />{/if}
    {#if hasClimateSel}
      {#if brushArmed}
        <PromptBanner
          message="Click and drag to paint. [ / ] resize · Esc cancels"
          oncancel={() => onArm(null)}
        />
      {/if}
      <BottomToolbar tools={climateTools} bind:armed={armedTool} onarm={onArm}>
        {#snippet children()}
          <span class="tool-lead">
            Painting {climateSelKey ?? (climateSelSlot === "zone" ? "Temperate" : "No winter")}
          </span>
        {/snippet}
        {#snippet extra()}
          {#if brushArmed}
            <MapBrush bind:size={brushSize} />
          {/if}
        {/snippet}
      </BottomToolbar>
    {/if}
  {/if}

  <!-- Simple Terrain: list-driven terrain_override painting (Sprint 11.2). -->
  {#if mode === "simple_terrain" && baseTerrain}
    <SimpleTerrainList
      categories={effectiveTerrainCategories}
      counts={terrainCounts}
      selectedKey={selectedTerrainKey}
      hover={terrainHover}
      onselect={selectTerrain}
      oncommitModifiers={commitTerrainModifiers}
    />
    {#if selectedTerrainKey}
      {#if brushArmed}
        <PromptBanner
          message="Click and drag to paint. [ / ] resize · Esc cancels"
          oncancel={() => onArm(null)}
        />
      {/if}
      <BottomToolbar tools={terrainTools} bind:armed={armedTool} onarm={onArm}>
        {#snippet children()}
          <span class="tool-lead">
            Painting {selectedTerrainKey === AUTO_KEY ? "Auto (erase override)" : selectedTerrainKey}
          </span>
        {/snippet}
        {#snippet extra()}
          {#if brushArmed}
            <MapBrush bind:size={brushSize} />
          {/if}
        {/snippet}
      </BottomToolbar>
    {/if}
  {/if}

  <!-- Trade Nodes: membership/location/route tools when a node is selected. -->
  {#if mode === "trade_nodes" && selectedNodeKey}
    {#if brushArmed}
      <PromptBanner
        message="Click and drag to paint membership. [ / ] resize · Esc cancels"
        oncancel={() => onArm(null)}
      />
    {/if}
    {#if armedTool === "tn_set_location"}
      <PromptBanner
        message={`Click a member province of ${selectedNodeKey} to set its collection point · Esc cancels`}
        oncancel={() => onArm(null)}
      />
    {/if}
    {#if armedTool === "tn_add_route"}
      <PromptBanner
        message="Click the target node's marker to add a route · Esc cancels"
        oncancel={() => onArm(null)}
      />
    {/if}
    <BottomToolbar tools={tradeNodeTools} bind:armed={armedTool} onarm={onArm}>
      {#snippet children()}
        <span class="tool-lead">Editing {selectedNodeKey}</span>
      {/snippet}
      {#snippet extra()}
        {#if brushArmed}
          <MapBrush bind:size={brushSize} />
        {/if}
      {/snippet}
    </BottomToolbar>
  {/if}

  <!-- Trade Nodes: create tool when nothing is selected. -->
  {#if mode === "trade_nodes" && !selectedNodeKey}
    {#if tradeFlowState.phase === "awaiting-click"}
      <PromptBanner
        message="Click the province where the trade node collects · Esc cancels"
        oncancel={cancelNodeFlow}
      />
    {/if}
    <BottomToolbar tools={createNodeTool} bind:armed={armedTool} onarm={onArm}>
      {#snippet children()}
        <span class="tool-lead">Trade Nodes</span>
      {/snippet}
    </BottomToolbar>
  {/if}

  {#if tradeFlowState.phase === "naming" && tradeFlowState.position}
    <InlineNamePrompt
      x={tradeFlowState.position.x}
      y={tradeFlowState.position.y}
      value={tradeFlowState.name ?? "New Trade Node"}
      label="Trade node"
      onaccept={acceptNodeName}
      oncancel={cancelNodeFlow}
    />
  {/if}

  <!-- Areas: membership brush when an area is selected. -->
  {#if mode === "areas" && selectedAreaKey}
    {#if brushArmed}
      <PromptBanner
        message="Click and drag to paint provinces into this area. [ / ] resize · Esc cancels"
        oncancel={() => onArm(null)}
      />
    {/if}
    <BottomToolbar tools={areaTools} bind:armed={armedTool} onarm={onArm}>
      {#snippet children()}
        <span class="tool-lead">Editing {selectedAreaKey}</span>
      {/snippet}
      {#snippet extra()}
        {#if brushArmed}
          <MapBrush bind:size={brushSize} />
        {/if}
      {/snippet}
    </BottomToolbar>
  {/if}

  <!-- Areas: create tool when nothing is selected. -->
  {#if mode === "areas" && !selectedAreaKey}
    {#if areaFlowState.phase === "awaiting-click"}
      <PromptBanner
        message="Click the starting province for the new area · Esc cancels"
        oncancel={cancelAreaFlow}
      />
    {/if}
    <BottomToolbar tools={createAreaTool} bind:armed={armedTool} onarm={onArm}>
      {#snippet children()}
        <span class="tool-lead">Areas</span>
      {/snippet}
    </BottomToolbar>
  {/if}

  {#if areaFlowState.phase === "naming" && areaFlowState.position}
    <InlineNamePrompt
      x={areaFlowState.position.x}
      y={areaFlowState.position.y}
      value={areaFlowState.name ?? "New Area"}
      label="Area"
      onaccept={acceptAreaName}
      oncancel={cancelAreaFlow}
    />
  {/if}

  <!-- Regions: area-granularity membership brush when a region is selected. -->
  {#if mode === "regions" && selectedRegionKey}
    {#if brushArmed}
      <PromptBanner
        message="Click and drag to paint whole areas into this region. [ / ] resize · Esc cancels"
        oncancel={() => onArm(null)}
      />
    {/if}
    <BottomToolbar tools={regionTools} bind:armed={armedTool} onarm={onArm}>
      {#snippet children()}
        <span class="tool-lead">Editing {selectedRegionKey}</span>
      {/snippet}
      {#snippet extra()}
        {#if brushArmed}
          <MapBrush bind:size={brushSize} />
        {/if}
      {/snippet}
    </BottomToolbar>
  {/if}

  <!-- Regions: create tool when nothing is selected. -->
  {#if mode === "regions" && !selectedRegionKey}
    {#if regionFlowState.phase === "awaiting-click"}
      <PromptBanner
        message="Click a province — its area becomes the new region's first member · Esc cancels"
        oncancel={cancelRegionFlow}
      />
    {/if}
    <BottomToolbar tools={createRegionTool} bind:armed={armedTool} onarm={onArm}>
      {#snippet children()}
        <span class="tool-lead">Regions</span>
      {/snippet}
    </BottomToolbar>
  {/if}

  {#if regionFlowState.phase === "naming" && regionFlowState.position}
    <InlineNamePrompt
      x={regionFlowState.position.x}
      y={regionFlowState.position.y}
      value={regionFlowState.name ?? "New Region"}
      label="Region"
      onaccept={acceptRegionName}
      oncancel={cancelRegionFlow}
    />
  {/if}

  <!-- Colonial Regions / Trade Companies: membership brush when one is selected. -->
  {#if isColonialMode && selectedColonialKey}
    {#if brushArmed}
      <PromptBanner
        message="Click and drag to paint provinces into this entry. [ / ] resize · Esc cancels"
        oncancel={() => onArm(null)}
      />
    {/if}
    <BottomToolbar tools={colonialTools} bind:armed={armedTool} onarm={onArm}>
      {#snippet children()}
        <span class="tool-lead">Editing {selectedColonialKey}</span>
      {/snippet}
      {#snippet extra()}
        {#if brushArmed}
          <MapBrush bind:size={brushSize} />
        {/if}
      {/snippet}
    </BottomToolbar>
  {/if}

  <!-- Colonial Regions / Trade Companies: create tool when nothing is selected. -->
  {#if isColonialMode && !selectedColonialKey}
    {#if colonialFlowState.phase === "awaiting-click"}
      <PromptBanner
        message="Click the starting province for the new {mode === 'trade_companies' ? 'trade company' : 'colonial region'} · Esc cancels"
        oncancel={cancelColonialFlow}
      />
    {/if}
    <BottomToolbar tools={createColonialTool} bind:armed={armedTool} onarm={onArm}>
      {#snippet children()}
        <span class="tool-lead">{mode === "trade_companies" ? "Trade Companies" : "Colonial Regions"}</span>
      {/snippet}
    </BottomToolbar>
  {/if}

  {#if colonialFlowState.phase === "naming" && colonialFlowState.position}
    <InlineNamePrompt
      x={colonialFlowState.position.x}
      y={colonialFlowState.position.y}
      value={colonialFlowState.name ?? (mode === "trade_companies" ? "New Trade Company" : "New Colonial Region")}
      label={mode === "trade_companies" ? "Trade company" : "Colonial region"}
      onaccept={acceptColonialName}
      oncancel={cancelColonialFlow}
    />
  {/if}

  <!-- Development: raise/lower airbrush (always available; no selection). -->
  {#if mode === "development"}
    {#if brushArmed}
      <PromptBanner
        message="Hold and drag to airbrush development. [ / ] resize · Esc cancels"
        oncancel={() => onArm(null)}
      />
    {/if}
    <BottomToolbar tools={devTools} bind:armed={armedTool} onarm={onArm}>
      {#snippet children()}
        <span class="tool-lead">Development</span>
      {/snippet}
      {#snippet extra()}
        {#if brushArmed}
          <MapBrush bind:size={brushSize} />
        {/if}
      {/snippet}
    </BottomToolbar>
  {/if}

  {#if (brushArmed || pcBrushArmed) && brushCursor.on && !dragging}
    <div
      class="brush-cursor"
      style="left: {brushCursor.x}px; top: {brushCursor.y}px; width: {brushCursor.d}px; height: {brushCursor.d}px;"
    ></div>
  {/if}

  {#if loading && bitmap}
    <div class="chip">Rendering…</div>
  {/if}

  {#if noticeMessage}
    <div class="chip notice">{noticeMessage}</div>
  {/if}

  {#if loading && !bitmap}
    <div class="overlay">
      <p>Rendering map…</p>
    </div>
  {:else if error}
    <div class="overlay">
      <p class="error">{error}</p>
      <button onclick={() => loadMap(!bitmap)}>Retry</button>
    </div>
  {/if}
</div>

<!-- Mass dynasty management (Sprint 1.3), opened from the Edit menu. -->
<DynastyModal bind:open={dynastiesOpen} mode="manage" {queue} {installPath} {modPath} />

{#each workspaceWindows() as workspaceWindow (workspaceWindow.id)}
  <WorkspaceWindow window={workspaceWindow}>
    {#snippet children(tab)}
      {#if tab.view.kind === "estates"}
        <EstatesOverlay
          {installPath}
          {modPath}
          date={selectedDate}
          {queue}
          focusKey={tab.view.focusKey ?? null}
          onfocused={() => openView({ kind: "estates" }, "reuse")}
          onopencountry={openCountryInPolitical}
        />
      {:else if tab.view.kind === "country"}
        <CountryPanel
          embedded
          {installPath}
          {modPath}
          {queue}
          {calendar}
          date={selectedDate}
          startDate={effectiveStart}
          tag={tab.view.tag}
          contentTab={tab.view.tab}
          onopenmissions={(tag) => openView({kind:"missions", tag})}
          onopenevents={() => openView({kind:"events"})}
          onopendecisions={() => openView({kind:"decisions"})}
          seed={countryScaffoldSeeds.get(tab.view.tag) ?? null}
          oncolor={onCountryColor}
          {capitalRequest}
          oncapitalapplied={() => (capitalRequest = null)}
          provNamePick={provNamePick}
          onarmprovnamepick={() => onArm("pick_prov_name")}
          onprovnamepickconsumed={() => (provNamePick = null)}
          onopencountry={openCountryInPolitical}
          onopenprovince={openProvince}
          onzoomtoprovince={centerOnProvince}
          onremovepending={removePendingCreatedCountry}
          onopennaming={(schemeKey) => openView({kind:"government-names", focusKey:schemeKey})}
          onopenestates={(key) => openView({ kind: "estates", focusKey: key }, "reuse")}
          onopenmechanics={(fam, key) => openView({kind:"mechanics", family:fam, focusKey:key})}
          onclose={() => closeTab(tab.id)}
        />
      {:else if tab.view.kind === "province"}
        <ProvincePanel
          embedded
          {installPath}
          {modPath}
          {queue}
          {calendar}
          date={selectedDate}
          startDate={effectiveStart}
          id={tab.view.id}
          contentTab={tab.view.tab}
          onclose={() => closeTab(tab.id)}
          onopencountry={openCountryInPolitical}
          onopenculture={openCultureMode}
          onopenmechanics={(fam, key) => openView({kind:"mechanics", family:fam, focusKey:key})}
        />
      {:else if tab.view.kind === "religion"}
        <ReligionPanel {installPath} {modPath} {queue} religionKey={tab.view.key} seed={createdSeed?.key === tab.view.key ? createdSeed : null} oncolor={onReligionColor} onjumpcountry={openCountryInPolitical} onjumpprovince={openProvince} onopenmechanics={(fam) => openView({kind:"mechanics",family:fam})} onclose={() => closeTab(tab.id)} />
      {:else if tab.view.kind === "culture"}
        <CulturePanel {installPath} {modPath} {queue} cultureKey={tab.view.key} seed={createdCultureSeed?.key === tab.view.key ? createdCultureSeed : null} oncolor={onCultureColor} onjumpcountry={openCountryInPolitical} onjumpprovince={openProvince} provNamePick={provNamePick} onarmprovnamepick={() => onArm("pick_prov_name")} onprovnamepickconsumed={() => (provNamePick = null)} onclose={() => closeTab(tab.id)} />
      {:else if tab.view.kind === "trade-node" && tradeNetwork}
        {@const key = tab.view.key}
        {@const node = tradeNetwork.nodes.find((x) => x.key === key)}
        {#if node}<TradeNodePanel {installPath} {modPath} {queue} network={tradeNetwork} {node} colorPresent={baseColorPresent.has(key) || createdNodeKeys.has(key)} mapH={tradeNetwork.map_height} issues={tradeIssues} {selectedRoute} onclose={() => closeTab(tab.id)} onselectnode={selectNodeByKey} onselectroute={selectRouteRef} onsetlocation={() => onArm("tn_set_location")} onaddroute={() => onArm("tn_add_route")} ondeleted={onNodeDeleted} onjump={tradeJump} onopenmechanics={(fam,key) => openView({kind:"mechanics",family:fam,focusKey:key})} />{/if}
      {:else if tab.view.kind === "area" && geoNetwork}
        {@const key = tab.view.key}
        {@const area = geoNetwork.areas.find((x) => x.key === key)}
        {#if area}<AreaPanel {queue} network={geoNetwork} {area} issues={geoIssues} onclose={() => closeTab(tab.id)} onjump={geoJump} ondeleted={onGeoDeleted} />{/if}
      {:else if tab.view.kind === "region" && geoNetwork}
        {@const key = tab.view.key}
        {@const region = geoNetwork.regions.find((x) => x.key === key)}
        {#if region}<RegionPanel {installPath} {modPath} {queue} network={geoNetwork} {region} issues={geoIssues} onclose={() => closeTab(tab.id)} onjumparea={jumpToAreaMode} onjump={geoJump} ondeleted={onGeoDeleted} />{/if}
      {:else if tab.view.kind === "colonial" && colonialData}
        {@const key = tab.view.key}
        {@const entry = colonialData.entries.find((x) => x.key === key)}
        {#if entry}<ColonialPanel {installPath} {modPath} {queue} data={colonialData} {entry} issues={selectedColonialIssues} onclose={() => closeTab(tab.id)} onjump={colonialJump} ondeleted={onColonialDeleted} onopenmechanics={(fam,key) => openView({kind:"mechanics",family:fam,focusKey:key})} />{/if}
      {:else if tab.view.kind === "adjacency" && effectiveAdj[tab.view.index]}
        {@const index = tab.view.index}
        <AdjacencyPanel row={effectiveAdj[index]} waterIds={adjWater} issues={adjIssues.filter((i) => i.row === index)} armed={armedTool} onchange={onAdjChange} onpickendpoint={(which) => onArm(which === "from" ? "adj_pick_from" : "adj_pick_to")} onpickthrough={() => onArm("adj_pick_through")} ondelete={deleteAdj} onclose={() => closeTab(tab.id)} />
      {:else if tab.view.kind === "climate" && climateModel}
        <ClimatePanel model={climateModel} counts={climateCountMap} selSlot={climateSelSlot} selKey={climateSelKey} mode={tab.view.key === "winter" ? "winter" : "climate"} bind:showWinterTint onselect={selectClimateEntry} />
      {:else if tab.view.kind === "decisions"}
        <DecisionsOverlay open {installPath} {modPath} {selectedDate} {queue} onjumpcountry={openCountryInPolitical} />
      {:else if tab.view.kind === "events"}
        <EventsOverlay open {installPath} {modPath} {selectedDate} {queue} onjumpcountry={openCountryInPolitical} />
      {:else if tab.view.kind === "missions"}
        <MissionsOverlay open {installPath} {modPath} {selectedDate} {queue} />
      {:else if tab.view.kind === "government-names"}
        <GovernmentNamesOverlay open {installPath} {modPath} {queue} focusKey={tab.view.focusKey ?? null} />
      {:else if tab.view.kind === "rebels"}
        <RebelsOverlay open {installPath} {modPath} date={selectedDate} {queue} onopenprovince={openProvince} />
      {:else if tab.view.kind === "mechanics"}
        <MechanicsOverlay open family={tab.view.family ?? null} focusKey={tab.view.focusKey ?? null} {installPath} {modPath} date={selectedDate} {queue} onopenevents={() => openView({kind:"events"})} onopennaming={() => openView({kind:"government-names"})} />
      {:else if tab.view.kind === "color-pools"}
        <ColorPoolsOverlay open {installPath} {modPath} {queue} />
      {:else if tab.view.kind === "empires"}
        <EmpiresOverlay open {installPath} {modPath} date={selectedDate} startDate={effectiveStart} {queue} onhighlightmembers={(ids) => { hreHighlightIds = ids ? new Set(ids) : null; if (mode === "political") updateHighlight(); }} onopenevents={() => openView({kind:"events"})} onopenreligion={() => setMode("religion")} />
      {:else if tab.view.kind === "technology"}
        <TechnologyOverlay open {installPath} {modPath} {queue} />
      {:else if tab.view.kind === "scripted"}
        <ScriptedOverlay open focusName={tab.view.focusKey ?? null} {installPath} {modPath} {queue} />
      {:else if tab.view.kind === "on-actions"}
        <OnActionsOverlay open {installPath} {modPath} {queue} />
      {:else if tab.view.kind === "localisation"}
        <LocalisationOverlay open {installPath} {modPath} {queue} />
      {:else if tab.view.kind === "defines"}
        <DefinesOverlay open {installPath} {modPath} {queue} />
      {:else if tab.view.kind === "problems"}
        <ProblemsOverlay open reports={problemsReports} running={problemsRunning} hasRun={problemsHasRun} onrerun={runProblems} onjump={problemsJump} />
      {:else if tab.view.kind === "search"}
        <SearchOverlay open {installPath} {modPath} onroute={searchJump} />
      {:else if tab.view.kind === "project-changes"}
        <ProjectChangesOverlay open {installPath} {modPath} />
      {:else if tab.view.kind === "edits"}
        <EditsPanel {queue} saved={savedComposites} onclose={() => closeTab(tab.id)} onjump={editJump} />
      {:else if tab.view.kind === "new-tab"}
        <NewTabView {installPath} {modPath} tabId={tab.id} onopen={openEntityFromPicker} />
      {:else if tab.view.kind === "shortcuts"}
        <ShortcutsView />
      {:else}
        <EmptyState title="Nothing to show yet" detail="This view has no matching entity in the current session, or its data is still loading." />
      {/if}
    {/snippet}
  </WorkspaceWindow>
{/each}

<!-- Problems dashboard (Sprint 30.2): aggregate validation across every domain. -->

<!-- Project-wide search (Sprint 30.3): Ctrl+Shift+F. -->

<!-- Mod-vs-base diff browser (Sprint 30.4): View ▸ Project Changes. -->

{#if workshop}
  <WorkshopModal
    {installPath}
    mode={workshop.mode}
    source={workshop.source}
    onclose={() => {
      workshop = null;
      workshopWarnPath = null;
    }}
    onforked={onWorkshopForked}
    onopenanyway={openWorkshopAnyway}
  />
{/if}

<style>
  .map-screen {
    position: fixed;
    inset: 0;
    background: var(--bg-0);
    overflow: hidden;
  }

  canvas {
    display: block;
    cursor: grab;
    touch-action: none;
  }

  canvas.hovering {
    cursor: pointer;
  }

  canvas.brushing,
  canvas.picking {
    cursor: crosshair;
  }

  canvas.grabbing {
    cursor: grabbing;
  }

  .brush-cursor {
    position: fixed;
    z-index: 9;
    transform: translate(-50%, -50%);
    border: 1.5px solid rgba(255, 255, 255, 0.9);
    border-radius: 50%;
    box-shadow: 0 0 0 1px rgba(0, 0, 0, 0.55);
    pointer-events: none;
  }

  .tool-lead {
    font-weight: 600;
    color: var(--text-1);
    white-space: nowrap;
  }

  .pc-confirm {
    padding: var(--sp-1) var(--sp-3);
    border: 1px solid var(--accent);
    border-radius: var(--r-1);
    background: var(--accent);
    color: var(--bg-0);
    font-weight: 600;
    cursor: pointer;
  }
  .pc-confirm:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .ng-check {
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
    margin-left: 0.6rem;
    font-size: 0.78rem;
    color: var(--text-1);
    white-space: nowrap;
    cursor: pointer;
  }
  .ng-check input {
    cursor: pointer;
  }

  .menu-backdrop {
    position: fixed;
    inset: 0;
    z-index: 5;
    border: none;
    background: transparent;
    cursor: default;
  }

  .menu {
    position: relative;
    z-index: 6;
  }

  .menu-btn {
    border: none;
    border-radius: 0;
    background: transparent;
    color: inherit;
    font-family: inherit;
    font-size: 0.9rem;
    padding: 0.3rem 0.8rem;
    cursor: pointer;
  }

  .menu-btn:hover,
  .menu-btn.open {
    background: var(--accent);
    color: var(--text-inverse);
  }

  .dropdown {
    position: absolute;
    top: 100%;
    left: 0;
    min-width: 13rem;
    display: flex;
    flex-direction: column;
    padding: 2px;
    border-radius: 0;
    background: var(--bg-3);
    border: 1px solid var(--bg-2);
    box-shadow: 2px 3px 8px rgba(0, 0, 0, 0.35);
  }

  .dropdown button {
    display: flex;
    justify-content: space-between;
    gap: 1rem;
    border: none;
    border-radius: 0;
    background: transparent;
    color: var(--text-1);
    font-family: inherit;
    font-size: 0.88rem;
    text-align: left;
    padding: 0.35rem 0.7rem;
    cursor: pointer;
    white-space: nowrap;
  }

  .dropdown button:hover:not(:disabled) {
    background: var(--accent);
    color: var(--text-inverse);
  }

  .dropdown button:disabled {
    color: var(--text-2);
    cursor: default;
  }

  .dropdown hr {
    border: none;
    border-top: 1px solid var(--bg-2);
    margin: 2px 0;
  }

  .shortcut {
    color: var(--text-2);
    font-size: 0.8rem;
  }

  .saved {
    color: var(--ok);
    font-size: 0.85rem;
    white-space: nowrap;
  }

  .title {
    font-weight: 700;
    white-space: nowrap;
  }

  .title-btn {
    background: none;
    border: 1px solid transparent;
    border-radius: var(--r-1);
    color: inherit;
    font: inherit;
    font-weight: 700;
    padding: 1px var(--sp-1);
    cursor: text;
  }

  .title-btn:hover {
    border-color: var(--border-strong);
    background: var(--bg-hover);
  }

  .title-edit {
    background: var(--bg-0);
    border: 1px solid var(--accent);
    border-radius: var(--r-1);
    color: var(--text-1);
    font: inherit;
    font-weight: 700;
    padding: 1px var(--sp-1);
    min-width: 14rem;
  }

  .path {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-2);
  }

  .zoom {
    min-width: 3.5rem;
    text-align: right;
    font-variant-numeric: tabular-nums;
    color: var(--text-2);
  }

  .modes {
    position: absolute;
    top: 3rem;
    left: 0.75rem;
    width: 12rem;
    max-height: calc(100vh - 5rem);
    display: flex;
    flex-direction: column;
    border-radius: 10px;
    background: rgba(20, 24, 29, 0.85);
    color: var(--text-1);
    backdrop-filter: blur(4px);
    /* Map-anchored chrome layer: above every map overlay canvas (5), below
       only the menu/toolbar + docked panels (10). */
    z-index: 9;
  }

  .modes h3 {
    margin: 0;
    padding: 0.6rem 0.85rem 0.4rem;
    font-size: 0.8rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-2);
  }

  .modes ul {
    list-style: none;
    margin: 0;
    padding: 0 0.4rem 0.4rem;
    overflow-y: auto;
  }

  .mode {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 0.4rem;
    text-align: left;
    border: none;
    border-radius: 6px;
    background: transparent;
    color: inherit;
    font-family: inherit;
    font-size: 0.9rem;
    padding: 0.35rem 0.5rem;
    cursor: pointer;
  }

  .mode-label {
    flex: 1;
    min-width: 0;
  }

  .view-only {
    flex: none;
    font-size: 0.62rem;
    line-height: 1;
    text-transform: uppercase;
    letter-spacing: 0.03em;
    padding: 0.12rem 0.3rem;
    border-radius: 3px;
    background: var(--accent);
    color: var(--text-inverse);
  }

  .mode:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.08);
  }

  .mode.active {
    background: rgba(57, 108, 216, 0.45);
  }

  .mode:disabled {
    cursor: default;
    opacity: 0.6;
  }

  /* View-menu checkbox rows: check glyph on the left, label following. */
  .dropdown button.toggle-item {
    justify-content: flex-start;
    gap: 0.4rem;
  }
  .dropdown button.toggle-item .check {
    display: inline-block;
    width: 0.9rem;
    text-align: center;
    color: var(--accent-text);
  }

  /* Count badge on View-menu entries (Edits queue size / Problems total). */
  .dropdown button .menu-badge {
    margin-left: auto;
    min-width: 1.2rem;
    text-align: center;
    padding: 0.02rem 0.35rem;
    font-size: 0.7rem;
    font-variant-numeric: tabular-nums;
    background: var(--accent);
    color: var(--text-inverse);
    border: 1px solid rgba(0, 0, 0, 0.35);
  }
  .dropdown button .menu-badge.err {
    background: var(--err);
  }

  /* Trade-detail hover tooltip (S3.3): follows the cursor over the map. */
  .trade-detail-tip {
    position: absolute;
    z-index: 20; /* popover: above overlay canvases (5) + map-anchored chrome (9) */
    max-width: 18rem;
    padding: 0.3rem 0.55rem;
    border: 1px solid var(--bg-2);
    border-radius: 4px;
    background: rgba(20, 24, 29, 0.92);
    color: var(--text-inverse);
    font-size: 0.78rem;
    pointer-events: none;
    white-space: nowrap;
  }
  .trade-detail-tip .tdt-row {
    line-height: 1.35;
  }

  .unassigned-toggle {
    position: absolute;
    top: 3rem;
    left: 13.25rem;
    z-index: 9; /* map-anchored chrome: above overlay canvases (5) */
    display: flex;
    align-items: center;
    gap: 0.35rem;
    padding: 0.3rem 0.6rem;
    border-radius: 6px;
    background: rgba(20, 24, 29, 0.85);
    color: var(--text-1);
    font-size: 0.8rem;
    cursor: pointer;
    user-select: none;
  }

  /* Bottom-anchored map chrome clears the BottomToolbar when one is mounted
     (it publishes its measured height as --bottom-toolbar-h). */
  .country-label {
    position: absolute;
    bottom: calc(1rem + var(--bottom-toolbar-h, 0px));
    left: 1rem;
    z-index: 9; /* map-anchored chrome: above overlay canvases (5) */
    padding: 0.35rem 0.9rem;
    border-radius: 6px;
    background: rgba(20, 24, 29, 0.85);
    color: var(--text-1);
    font-size: 1rem;
    font-weight: 600;
    pointer-events: none;
    white-space: nowrap;
  }
  .occupied-by {
    margin-left: 0.6rem;
    font-size: 0.8rem;
    font-weight: 500;
    color: var(--warn);
  }

  .chip {
    position: absolute;
    bottom: calc(1rem + var(--bottom-toolbar-h, 0px));
    left: 50%;
    transform: translateX(-50%);
    padding: 0.4rem 1rem;
    border-radius: 999px;
    background: rgba(20, 24, 29, 0.85);
    color: var(--text-1);
    font-size: 0.85rem;
  }

  .chip.notice {
    bottom: calc(3rem + var(--bottom-toolbar-h, 0px));
    background: rgba(63, 72, 85, 0.95);
    border: 1px solid var(--warn);
    color: var(--warn);
  }

  /* --- Date selector chip (Sprint 12.1) --- */
  .date-chip-wrap {
    position: absolute;
    top: 3rem;
    right: 0.75rem;
    /* Map-anchored chrome: above overlay canvases (5), below toolbar (10). */
    z-index: 9;
  }

  .date-chip {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    padding: 0.35rem 0.7rem;
    border: 1px solid var(--bg-2);
    border-radius: 0;
    background: var(--bg-3);
    color: var(--text-1);
    font-family: inherit;
    font-size: 0.85rem;
    cursor: pointer;
    box-shadow: 0 2px 6px rgba(0, 0, 0, 0.35);
    white-space: nowrap;
  }

  .date-chip:hover {
    background: var(--accent);
    color: var(--text-inverse);
  }

  .date-chip.busy {
    cursor: progress;
  }

  .cal-glyph {
    font-size: 0.9rem;
    line-height: 1;
  }

  .date-text {
    font-variant-numeric: tabular-nums;
    font-weight: 600;
  }

  .caret {
    font-size: 0.7rem;
    color: var(--text-2);
  }

  .date-spinner {
    width: 0.8rem;
    height: 0.8rem;
    border: 2px solid rgba(207, 212, 219, 0.35);
    border-top-color: var(--text-1);
    border-radius: 50%;
    animation: date-spin 0.7s linear infinite;
  }

  @keyframes date-spin {
    to {
      transform: rotate(360deg);
    }
  }

  .date-backdrop {
    position: fixed;
    inset: 0;
    z-index: 9;
    border: none;
    background: transparent;
    cursor: default;
  }

  .date-menu {
    position: absolute;
    top: calc(100% + 3px);
    right: 0;
    z-index: 10;
    width: 19rem;
    display: flex;
    flex-direction: column;
    background: var(--bg-3);
    border: 1px solid var(--bg-2);
    box-shadow: 2px 3px 8px rgba(0, 0, 0, 0.4);
  }

  .date-menu-head {
    padding: 0.45rem 0.7rem 0.3rem;
    font-size: 0.72rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-2);
  }

  .bookmark-list {
    list-style: none;
    margin: 0;
    padding: 0 2px;
    max-height: 16rem;
    overflow-y: auto;
  }

  .bookmark-row {
    width: 100%;
    display: grid;
    grid-template-columns: 1.1rem 1fr auto;
    align-items: center;
    gap: 0.5rem;
    border: none;
    border-radius: 0;
    background: transparent;
    color: var(--text-1);
    font-family: inherit;
    font-size: 0.85rem;
    text-align: left;
    padding: 0.35rem 0.5rem;
    cursor: pointer;
  }

  .bookmark-row:hover {
    background: var(--accent);
    color: var(--text-inverse);
  }

  .bookmark-row.current {
    background: rgba(74, 109, 167, 0.35);
  }

  .bookmark-row .check {
    color: var(--ok);
    text-align: center;
  }

  .bm-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .bm-date {
    color: var(--text-2);
    font-variant-numeric: tabular-nums;
    font-size: 0.78rem;
    white-space: nowrap;
  }

  .bookmark-row:hover .bm-date {
    color: var(--text-1);
  }

  .bookmark-list .empty {
    padding: 0.5rem;
    color: var(--text-2);
    font-size: 0.82rem;
  }

  .range-warn {
    display: flex;
    align-items: flex-start;
    gap: 0.4rem;
    margin: 0 2px 0.25rem;
    padding: 0.35rem 0.5rem;
    background: rgba(161, 102, 47, 0.22);
    border: 1px solid var(--warn);
    color: var(--warn);
    font-size: 0.74rem;
    line-height: 1.3;
  }

  .rw-icon {
    color: var(--warn);
    line-height: 1.2;
  }

  .bookmark-line {
    display: flex;
    flex-direction: column;
  }

  .bookmark-line.oor .bookmark-row {
    color: var(--warn);
  }

  .extend-btn {
    align-self: flex-end;
    margin: 0 0.5rem 0.25rem;
    border: 1px solid var(--warn);
    background: var(--bg-3);
    color: var(--warn);
    font-family: inherit;
    font-size: 0.72rem;
    padding: 0.1rem 0.5rem;
    cursor: pointer;
  }

  .extend-btn:hover {
    background: var(--warn);
    color: var(--text-inverse);
  }

  .date-menu hr {
    border: none;
    border-top: 1px solid var(--bg-2);
    margin: 2px 0;
  }

  .new-date-btn {
    border: none;
    border-radius: 0;
    background: transparent;
    color: var(--text-1);
    font-family: inherit;
    font-size: 0.85rem;
    text-align: left;
    padding: 0.45rem 0.7rem;
    cursor: pointer;
  }

  .new-date-btn:hover {
    background: var(--accent);
    color: var(--text-inverse);
  }

  .new-date-form {
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
    padding: 0.5rem 0.7rem 0.6rem;
  }

  .nd-label {
    font-size: 0.72rem;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-2);
  }

  .nd-name {
    background: var(--bg-1);
    border: 1px solid var(--border);
    color: var(--text-1);
    font-family: inherit;
    font-size: 0.85rem;
    padding: 0.3rem 0.4rem;
    outline: none;
  }

  .nd-hint {
    font-size: 0.75rem;
    color: var(--warn);
    line-height: 1.3;
  }

  .nd-error {
    font-size: 0.78rem;
    color: var(--err);
  }

  .nd-actions {
    display: flex;
    gap: 0.4rem;
    margin-top: 0.15rem;
  }

  .nd-ok,
  .nd-cancel {
    border: 1px solid var(--bg-2);
    border-radius: 0;
    background: var(--bg-2);
    color: var(--text-1);
    font-family: inherit;
    font-size: 0.82rem;
    padding: 0.3rem 0.7rem;
    cursor: pointer;
  }

  .nd-ok:hover {
    background: var(--accent);
    color: var(--text-inverse);
  }

  .nd-cancel:hover {
    background: var(--danger-bg);
    color: var(--text-inverse);
  }

  .static-note {
    position: absolute;
    top: 3.5rem;
    left: 50%;
    transform: translateX(-50%);
    z-index: 9;
    max-width: 32rem;
    padding: 0.4rem 0.9rem;
    background: var(--bg-3);
    border: 1px solid var(--warn);
    color: var(--warn);
    font-size: 0.82rem;
    box-shadow: 0 3px 10px rgba(0, 0, 0, 0.4);
  }

  .overlay {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 1rem;
    color: var(--text-1);
    background: rgba(20, 24, 29, 0.6);
  }

  .overlay .error {
    max-width: 40rem;
    color: var(--err);
    text-align: center;
  }

  .overlay button {
    border: 1px solid var(--border-strong);
    border-radius: 6px;
    background: transparent;
    color: inherit;
    font-family: inherit;
    padding: 0.4rem 1rem;
    cursor: pointer;
  }
</style>
