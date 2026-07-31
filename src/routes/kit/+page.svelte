<!--
  /kit — DEV BENCH for the Phase 0.6 shared UI kit. This is NOT shipped UI: it exists
  only to exercise every kit component with live sample data (visual + interaction
  test bench). Real consumers wire these into MapView/panels in Sprint 1+.

  It drives every interactive path: tabbed SidePanel, ListSection count badge, the
  searchable dropdown with swatches, the multi-select modal (opened via a button, with
  a mass-delete action), the typed modifier editor (percent/flat/boolean/free-text),
  single + 3-color pickers, the date picker, a horizontal SliderGroup with locks and a
  vertical one, and an armed BottomToolbar tool driving createEntityFlow through its
  prompt-banner → inline-name-prompt → done sequence.
-->
<script lang="ts">
  import {
    SidePanel,
    ListSection,
    SearchDropdown,
    MultiSelectModal,
    ModifierEditor,
    ColorPicker,
    DatePicker,
    SliderGroup,
    BottomToolbar,
    PromptBanner,
    InlineNamePrompt,
    createEntityFlow,
  } from "$lib/components/ui";
  import type {
    DropdownItem,
    MultiSelectItem,
    KnownModifier,
    ModifierRow,
    RGB,
    ToolButton,
    EntityFlowState,
  } from "$lib/components/ui";
  import IconOverlay from "$lib/components/IconOverlay.svelte";
  import {
    computeCentroids,
    DEFAULT_CONFIG,
    type OverlayItem,
    type Atlas,
  } from "$lib/overlay";
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import ValidationStrip, {
    type ValidationIssue,
    type JumpTarget,
  } from "$lib/components/ValidationStrip.svelte";

  // --- SidePanel ---
  const tabs = [
    { id: "country", label: "Country" },
    { id: "diplomacy", label: "Diplomacy" },
  ];
  let activeTab = $state("country");

  // --- SearchDropdown: a fake tag/country picker with color swatches ---
  // Demonstration data swatches and canvas paint below are intentionally literal.
  const tagItems: DropdownItem[] = [
    { key: "FRA", label: "France", swatch: "#2b6cb0" },
    { key: "ENG", label: "England", swatch: "#c53030" },
    { key: "CAS", label: "Castile", swatch: "#d69e2e" },
    { key: "OTT", label: "Ottomans", swatch: "#2f855a" },
    { key: "MOS", label: "Muscovy", swatch: "#6b46c1" },
    { key: "SWE", label: "Sweden", swatch: "#3182ce" },
    { key: "PAP", label: "Papal State", swatch: "#e2e8f0" },
    { key: "MAM", label: "Mamluks", swatch: "#dd6b20" },
  ];
  let selectedTag = $state<string | null>("FRA");

  // --- MultiSelectModal: a fake dynasty modal with usage counts + mass delete ---
  const dynastyItems: MultiSelectItem[] = [
    { key: "valois", label: "de Valois", badge: 12 },
    { key: "habsburg", label: "von Habsburg", badge: 34 },
    { key: "trastamara", label: "de Trastámara", badge: 8 },
    { key: "jagiellon", label: "Jagiellon", badge: 15 },
    { key: "osman", label: "Osman", badge: 3 },
    { key: "rurikovich", label: "Rurikovich", badge: 21 },
    { key: "aviz", label: "de Avis", badge: 5 },
  ];
  let modalOpen = $state(false);
  let dynastySelection = $state<string[]>(["habsburg"]);
  let lastMassAction = $state("");
  let confirmedDynasties = $state<string[]>([]);

  // --- ModifierEditor ---
  const knownModifiers: KnownModifier[] = [
    { key: "discipline", label: "Discipline", kind: "percent" },
    { key: "land_morale", label: "Land Morale", kind: "percent" },
    { key: "tax_income", label: "Yearly Tax Income", kind: "flat" },
    { key: "global_manpower", label: "National Manpower", kind: "flat" },
    { key: "may_recruit_female_generals", label: "Female Generals", kind: "boolean" },
    { key: "tolerance_own", label: "Tolerance of the True Faith", kind: "flat" },
  ];
  let modifiers = $state<ModifierRow[]>([
    { key: "discipline", value: "0.05" },
    { key: "tax_income", value: "1" },
    { key: "some_mod_custom_thing", value: "yes" },
  ]);

  // --- ColorPicker ---
  let mapColor = $state<RGB>({ r: 43, g: 108, b: 176 });
  let revColors = $state<RGB[]>([
    { r: 200, g: 30, b: 30 },
    { r: 240, g: 240, b: 240 },
    { r: 30, g: 30, b: 200 },
  ]);

  // --- DatePicker ---
  let startDate = $state("1444.11.11");

  // --- SliderGroup: dev mix (sums to 100), plus a vertical variant ---
  let devMix = $state([34, 33, 33]);
  let devLocks = $state([false, false, false]);
  const devLabels = ["Base Tax", "Production", "Manpower"];

  let vMix = $state([50, 25, 25]);
  let vLocks = $state([false, false, false]);

  // --- BottomToolbar + createEntityFlow ---
  const tools: ToolButton[] = [
    { id: "create-country", label: "Create Country", icon: "➕", tooltip: "Create a new country" },
    { id: "add-province", label: "Add Province", icon: "🖌", tooltip: "Paint provinces" },
  ];
  let armedTool = $state<string | null>(null);

  interface ScaffoldArgs {
    provinceId: number;
    name: string;
  }

  const flow = createEntityFlow<number, ScaffoldArgs>({
    tool: "create-country",
    defaultName: () => "New Country",
    buildArgs: (provinceId, name) => ({ provinceId, name }),
  });

  let flowState = $state<EntityFlowState<number, ScaffoldArgs>>(flow.state);
  let flowResult = $state("");

  $effect(() => flow.subscribe((s) => (flowState = s)));

  function onArm(id: string | null) {
    flow.reset();
    flowResult = "";
    if (id === "create-country") flow.arm();
  }

  // Fake "map": clicking while awaiting a click advances the flow.
  function onFakeMapClick(e: MouseEvent) {
    if (flowState.phase !== "awaiting-click") return;
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const provinceId = 1000 + Math.floor(Math.random() * 8000);
    flow.mapClicked(provinceId, {
      x: e.clientX - rect.left,
      y: e.clientY - rect.top,
    });
  }

  function acceptName(name: string) {
    const args = flow.submitName(name);
    if (args) {
      flowResult = `Scaffolded "${args.name}" at province #${args.provinceId}`;
    }
    armedTool = null;
    flow.reset();
  }

  function cancelFlow() {
    flow.cancel();
    armedTool = null;
    flow.reset();
  }

  // --- ValidationStrip (Phase 0.9) ---
  const sampleIssues: ValidationIssue[] = [
    {
      severity: "error",
      message: "Trade node 'genoa' forms a cycle with 'venice'",
      jump: { kind: "node", id: "genoa" },
    },
    {
      severity: "warning",
      message: 'Area "netherlands_area" has no provinces',
      jump: { kind: "area", id: "netherlands_area" },
    },
    {
      severity: "warning",
      message: "Land province 2751 (Rhodes) is not assigned to any area",
      jump: { kind: "province", id: 2751 },
    },
    {
      severity: "info",
      message: "Impassable province 1122 has an owner (MOR)",
      jump: { kind: "province", id: 1122 },
    },
    { severity: "info", message: "A note with no jump target", jump: null },
  ];
  let lastJump = $state("");
  function onJump(j: JumpTarget) {
    lastJump = `${j.kind}: ${j.id}`;
  }

  // Live run against a real install. We can't register commands here, so we
  // just invoke `validate` and fall back gracefully if the orchestrator hasn't
  // registered it yet (or the hardcoded install path isn't present).
  const DEFAULT_INSTALL =
    "C:\\Program Files (x86)\\Steam\\steamapps\\common\\Europa Universalis IV";
  let realIssues = $state<ValidationIssue[]>([]);
  let validationStatus = $state("");
  let running = $state(false);

  async function runAreasValidation() {
    running = true;
    validationStatus = "";
    try {
      const result = await invoke<ValidationIssue[]>("validate", {
        domain: "areas",
        installPath: DEFAULT_INSTALL,
        modPath: null,
      });
      realIssues = result;
      validationStatus = `Loaded ${result.length} issue(s) from the real install.`;
    } catch (e) {
      const msg = String(e);
      realIssues = [];
      if (/not found|not registered|unknown command|allowlist/i.test(msg)) {
        validationStatus =
          "The 'validate' command isn't registered yet — showing sample issues only. (The orchestrator wires it into lib.rs in a later step.)";
      } else {
        validationStatus = `Validation could not run: ${msg}`;
      }
    } finally {
      running = false;
    }
  }

  // --- IconOverlay (Phase 0.7) -------------------------------------------------
  // A fully synthetic bench: a 200×150 fake province-id buffer (a few blobs incl.
  // a crescent), a canvas-generated fake icon strip, and pan/zoom sliders driving
  // the same {scale, offsetX, offsetY} transform MapView feeds the overlay. This
  // proves centroids (icons land inside blobs), crescent snapping (icon 2 sits on
  // the arc, not in the gap), the zoom-threshold fade, and viewport culling.
  const OMAPW = 200;
  const OMAPH = 150;
  const OV_W = 480; // overlay viewport CSS size
  const OV_H = 360;

  // Colors used to paint each blob on the base canvas (id → RGB).
  const BLOB_PAL: Record<number, [number, number, number]> = {
    1: [70, 96, 138],
    2: [140, 74, 74],
    3: [74, 138, 96],
    4: [140, 116, 60],
    5: [116, 74, 138],
  };

  function buildIdBuffer(): Uint16Array {
    const ids = new Uint16Array(OMAPW * OMAPH).fill(0xffff);
    const set = (x: number, y: number, id: number) => {
      if (x >= 0 && x < OMAPW && y >= 0 && y < OMAPH) ids[y * OMAPW + x] = id;
    };
    // id 1 — rectangle (centroid trivially interior).
    for (let y = 22; y < 70; y++) for (let x = 16; x < 72; x++) set(x, y, 1);
    // id 2 — disc.
    const cx = 122,
      cy = 44,
      cr = 28;
    for (let y = cy - cr; y <= cy + cr; y++)
      for (let x = cx - cr; x <= cx + cr; x++)
        if ((x - cx) ** 2 + (y - cy) ** 2 <= cr * cr) set(x, y, 2);
    // id 3 — crescent (disc minus an offset disc): its mean pixel lands in the
    // concavity, OUTSIDE the shape — the snap must pull the icon onto the arc.
    const bx = 58,
      by = 116,
      br = 36,
      kx = 80,
      ky = 110,
      kr = 32;
    for (let y = by - br; y <= by + br; y++)
      for (let x = bx - br; x <= bx + br; x++) {
        const inB = (x - bx) ** 2 + (y - by) ** 2 <= br * br;
        const inK = (x - kx) ** 2 + (y - ky) ** 2 <= kr * kr;
        if (inB && !inK) set(x, y, 3);
      }
    // id 4 — blob far to the right (scrolls out of view → culling).
    for (let y = 96; y < 138; y++) for (let x = 150; x < 192; x++) set(x, y, 4);
    // id 5 — tiny island (precise centroid on a small shape → gets a label).
    for (let y = 16; y < 22; y++) for (let x = 150; x < 158; x++) set(x, y, 5);
    return ids;
  }

  async function buildAtlas(): Promise<Atlas> {
    const N = 6;
    const F = 40;
    const c = document.createElement("canvas");
    c.width = F * N;
    c.height = F;
    const ctx = c.getContext("2d")!;
    const colors = ["#e05a5a", "#5ad0e0", "#e0c85a", "#8a5ae0", "#5ae08a", "#e08a5a"];
    for (let i = 0; i < N; i++) {
      ctx.fillStyle = colors[i];
      ctx.beginPath();
      ctx.arc(i * F + F / 2, F / 2, F / 2 - 4, 0, Math.PI * 2);
      ctx.fill();
      ctx.fillStyle = "#14181d";
      ctx.font = "bold 20px sans-serif";
      ctx.textAlign = "center";
      ctx.textBaseline = "middle";
      ctx.fillText(String(i), i * F + F / 2, F / 2 + 1);
    }
    return { image: await createImageBitmap(c), frameW: F, frameH: F, count: N };
  }

  let ovIds = $state<Uint16Array | null>(null);
  let ovAtlas = $state<Atlas | null>(null);
  let ovScale = $state(3);
  let ovOffX = $state(24);
  let ovOffY = $state(18);
  let showStatBoxes = $state(false);
  let ovView = $derived({ scale: ovScale, offsetX: ovOffX, offsetY: ovOffY });

  // Icons on the blobs; a label on the tiny island. Toggling "stat boxes" swaps
  // every entry for a 3-line stat box to exercise that item type too.
  let ovItems = $derived.by(() => {
    const m = new Map<number, OverlayItem>();
    if (showStatBoxes) {
      m.set(1, { statBox: ["T 3", "P 2", "M 1"] });
      m.set(2, { statBox: ["T 5", "P 4", "M 2"] });
      m.set(3, { statBox: ["T 1", "P 1", "M 6"] });
      m.set(4, { statBox: ["T 2", "P 3", "M 3"] });
      m.set(5, { statBox: ["T 1", "P 0", "M 1"] });
    } else {
      m.set(1, { iconIndex: 0 });
      m.set(2, { iconIndex: 1 });
      m.set(3, { iconIndex: 2 });
      m.set(4, { iconIndex: 3 });
      m.set(5, { label: "Isle" });
    }
    return m;
  });

  // Live centroid readout: proves each centroid is interior to its blob.
  let ovCentroidReport = $derived.by(() => {
    if (!ovIds) return "";
    const cents = computeCentroids(ovIds, OMAPW, OMAPH);
    return [...cents.entries()]
      .sort((a, b) => a[0] - b[0])
      .map(([id, p]) => {
        const inside = ovIds![Math.round(p.y) * OMAPW + Math.round(p.x)] === id;
        return `#${id}→(${p.x.toFixed(0)},${p.y.toFixed(0)})${inside ? "✓" : "✗"}`;
      })
      .join("  ");
  });

  let ovBaseCanvas: HTMLCanvasElement;
  let ovOffscreen: HTMLCanvasElement | null = null;

  function buildOffscreen(ids: Uint16Array) {
    const c = document.createElement("canvas");
    c.width = OMAPW;
    c.height = OMAPH;
    const ctx = c.getContext("2d")!;
    const img = ctx.createImageData(OMAPW, OMAPH);
    for (let i = 0; i < ids.length; i++) {
      const id = ids[i];
      const p = i * 4;
      const col = id === 0xffff ? [26, 30, 36] : (BLOB_PAL[id] ?? [80, 80, 80]);
      img.data[p] = col[0];
      img.data[p + 1] = col[1];
      img.data[p + 2] = col[2];
      img.data[p + 3] = 255;
    }
    ctx.putImageData(img, 0, 0);
    ovOffscreen = c;
  }

  // Draw the fake "map" (colored blobs) under the overlay, using the SAME
  // transform the overlay uses so icons and blobs stay registered.
  function drawOvBase() {
    if (!ovBaseCanvas || !ovOffscreen) return;
    const dpr = window.devicePixelRatio || 1;
    ovBaseCanvas.width = Math.round(OV_W * dpr);
    ovBaseCanvas.height = Math.round(OV_H * dpr);
    const ctx = ovBaseCanvas.getContext("2d")!;
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.fillStyle = "#0e1116";
    ctx.fillRect(0, 0, ovBaseCanvas.width, ovBaseCanvas.height);
    ctx.setTransform(ovScale * dpr, 0, 0, ovScale * dpr, ovOffX * dpr, ovOffY * dpr);
    ctx.imageSmoothingEnabled = false;
    ctx.drawImage(ovOffscreen, 0, 0);
  }

  $effect(() => {
    // Re-draw the base whenever the transform changes (overlay redraws itself).
    void ovScale;
    void ovOffX;
    void ovOffY;
    void ovIds;
    drawOvBase();
  });

  onMount(async () => {
    const ids = buildIdBuffer();
    buildOffscreen(ids);
    ovIds = ids;
    drawOvBase();
    ovAtlas = await buildAtlas();
  });
</script>

<div class="bench">
  <header class="bench-head">
    <h1>UI Kit Bench</h1>
    <span class="note">Dev-only test bench for Phase 0.6 components — not shipped UI.</span>
  </header>

  <div class="grid">
    <section class="card">
      <h2>SearchDropdown</h2>
      <SearchDropdown items={tagItems} bind:value={selectedTag} placeholder="Pick a country…" />
      <p class="out">Selected: <code>{selectedTag ?? "none"}</code></p>
    </section>

    <section class="card">
      <h2>MultiSelectModal</h2>
      <button class="demo-btn" onclick={() => (modalOpen = true)}>Choose Dynasty…</button>
      <p class="out">Confirmed: <code>{confirmedDynasties.join(", ") || "none"}</code></p>
      {#if lastMassAction}<p class="out">Mass action: <code>{lastMassAction}</code></p>{/if}
    </section>

    <section class="card">
      <h2>ColorPicker — single</h2>
      <ColorPicker bind:value={mapColor} label="Map color" />
      <p class="out">rgb({mapColor.r}, {mapColor.g}, {mapColor.b})</p>
    </section>

    <section class="card">
      <h2>ColorPicker — 3-color (revolutionary)</h2>
      <ColorPicker bind:values={revColors} label="Rev. colors" />
      <p class="out">{revColors.map((c) => `${c.r},${c.g},${c.b}`).join(" · ")}</p>
    </section>

    <section class="card">
      <h2>DatePicker</h2>
      <DatePicker bind:value={startDate} min="1.1.1" max="9999.1.1" />
      <p class="out">Value: <code>{startDate}</code></p>
    </section>

    <section class="card">
      <h2>SliderGroup — horizontal, locks</h2>
      <SliderGroup
        bind:values={devMix}
        bind:locks={devLocks}
        labels={devLabels}
        total={100}
        format={(v) => `${Math.round(v)}%`}
      />
      <p class="out">Sum: {Math.round(devMix.reduce((a, b) => a + b, 0))}</p>
    </section>

    <section class="card">
      <h2>SliderGroup — vertical</h2>
      <SliderGroup
        bind:values={vMix}
        bind:locks={vLocks}
        labels={devLabels}
        total={100}
        orientation="vertical"
        format={(v) => `${Math.round(v)}`}
      />
    </section>

    <section class="card wide">
      <h2>ModifierEditor</h2>
      <ModifierEditor bind:modifiers known={knownModifiers} />
      <p class="out">{modifiers.map((m) => `${m.key}=${m.value}`).join("  ")}</p>
    </section>

    <section class="card wide">
      <h2>ListSection</h2>
      <ListSection title="Alliances" count={5}>
        {#each ["Castile", "Aragon", "Portugal", "Naples", "Provence"] as name}
          <div class="list-row">{name}</div>
        {/each}
      </ListSection>
    </section>

    <section class="card wide">
      <h2>ValidationStrip — sample issues (collapsible, click to jump)</h2>
      <ValidationStrip issues={sampleIssues} onjump={onJump} />
      <p class="out">Last jump: <code>{lastJump || "none"}</code></p>
    </section>

    <section class="card wide">
      <h2>ValidationStrip — live "areas" domain</h2>
      <button class="demo-btn" onclick={runAreasValidation} disabled={running}>
        {running ? "Running…" : "Run areas validation"}
      </button>
      {#if validationStatus}<p class="out">{validationStatus}</p>{/if}
      <ValidationStrip
        issues={realIssues}
        onjump={onJump}
        title="Areas"
        emptyLabel="Run the check to see real results"
      />
    </section>

    <section class="card wide">
      <h2>IconOverlay — per-province icons / labels / stat boxes (Phase 0.7)</h2>
      <div class="ov-controls">
        <label>
          Zoom <code>{ovScale.toFixed(2)}×</code>
          <input type="range" min="0.5" max="8" step="0.05" bind:value={ovScale} />
        </label>
        <label>
          Pan X <code>{Math.round(ovOffX)}</code>
          <input type="range" min="-600" max="480" step="1" bind:value={ovOffX} />
        </label>
        <label>
          Pan Y <code>{Math.round(ovOffY)}</code>
          <input type="range" min="-400" max="360" step="1" bind:value={ovOffY} />
        </label>
        <label class="chk">
          <input type="checkbox" bind:checked={showStatBoxes} /> Stat boxes
        </label>
      </div>
      <p class="out">
        Fade threshold {DEFAULT_CONFIG.fadeStart}×–{DEFAULT_CONFIG.fadeEnd}× — below it
        icons are hidden; drag Zoom across it to fade. Pan #4 (right blob) off-screen to
        see culling.
      </p>
      <div class="ov-stage" style="width:{OV_W}px; height:{OV_H}px;">
        <canvas bind:this={ovBaseCanvas} class="ov-base" style="width:{OV_W}px; height:{OV_H}px;"
        ></canvas>
        <IconOverlay
          provinceIds={ovIds}
          mapW={OMAPW}
          mapH={OMAPH}
          view={ovView}
          cssWidth={OV_W}
          cssHeight={OV_H}
          items={ovItems}
          atlas={ovAtlas}
          config={DEFAULT_CONFIG}
        />
      </div>
      <p class="out">Centroids (✓ = interior-snapped): <code>{ovCentroidReport}</code></p>
    </section>
  </div>

  <!-- Fake map surface for the create-entity flow -->
  <section class="card map-card">
    <h2>BottomToolbar + createEntityFlow — phase: <code>{flowState.phase}</code></h2>
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="fake-map" onclick={onFakeMapClick}>
      <span class="map-hint">
        {flowState.phase === "awaiting-click"
          ? "Click anywhere here to place the capital"
          : "Arm a tool from the bottom toolbar"}
      </span>

      {#if flowState.phase === "awaiting-click"}
        <PromptBanner message="Click on capital province" oncancel={cancelFlow} />
      {/if}

      {#if flowState.phase === "naming" && flowState.position}
        <InlineNamePrompt
          x={flowState.position.x}
          y={flowState.position.y}
          value={flowState.name ?? ""}
          label="Country"
          onaccept={acceptName}
          oncancel={cancelFlow}
        />
      {/if}
    </div>
    {#if flowResult}<p class="out ok">{flowResult}</p>{/if}
  </section>

  <!-- The tabbed side panel floats over everything on the right, like in-app -->
  <SidePanel title="France" {tabs} bind:activeTab>
    {#snippet header()}
      <span class="panel-chip">FRA · Kingdom</span>
    {/snippet}
    {#if activeTab === "country"}
      <p class="panel-body">Country tab — identity, ruler, ideas live here in Sprint 1.</p>
      <ColorPicker bind:value={mapColor} label="Color" />
    {:else}
      <ListSection title="Subjects" count={2}>
        <div class="list-row">Provence (vassal)</div>
        <div class="list-row">Navarra (PU junior)</div>
      </ListSection>
    {/if}
  </SidePanel>

  <BottomToolbar {tools} bind:armed={armedTool} onarm={onArm} />

  <MultiSelectModal
    bind:open={modalOpen}
    title="Choose Dynasty"
    items={dynastyItems}
    bind:selected={dynastySelection}
    confirmLabel="Apply"
    onconfirm={(keys) => (confirmedDynasties = keys)}
  >
    {#snippet actions(sel)}
      <button
        class="danger"
        disabled={sel.length === 0}
        onclick={() => (lastMassAction = `Deleted ${sel.length} dynastie(s)`)}
      >
        Delete
      </button>
    {/snippet}
  </MultiSelectModal>
</div>

<style>
  .bench {
    min-height: 100vh;
    background: var(--bg-0);
    color: var(--text-1);
    font-family: Inter, system-ui, sans-serif;
    padding: 1rem 22rem 4rem 1rem; /* right padding clears the floating SidePanel */
  }

  .bench-head {
    display: flex;
    align-items: baseline;
    gap: 1rem;
    margin-bottom: 1rem;
  }

  h1 {
    margin: 0;
    font-size: 1.3rem;
  }

  .note {
    color: var(--text-2);
    font-size: 0.85rem;
  }

  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(19rem, 1fr));
    gap: 0.75rem;
  }

  .card {
    background: var(--bg-2);
    border: 1px solid var(--border);
    padding: 0.75rem;
  }

  .card.wide {
    grid-column: 1 / -1;
  }

  .map-card {
    margin-top: 0.75rem;
  }

  h2 {
    margin: 0 0 0.6rem;
    font-size: 0.85rem;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-2);
  }

  .out {
    margin: 0.5rem 0 0;
    font-size: 0.8rem;
    color: var(--text-2);
  }

  .out.ok {
    color: var(--ok);
  }

  code {
    color: var(--text-1);
    background: var(--bg-1);
    padding: 0 0.25rem;
  }

  .demo-btn,
  .danger {
    border: 1px solid var(--border);
    background: var(--bg-3);
    color: var(--text-1);
    font-family: inherit;
    font-size: 0.83rem;
    padding: 0.3rem 0.7rem;
    cursor: pointer;
  }

  .demo-btn:hover {
    background: var(--accent);
    color: var(--text-inverse);
  }

  .danger {
    background: var(--danger-bg);
    color: var(--text-inverse);
  }

  .danger:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .list-row {
    padding: 0.3rem 0.45rem;
    font-size: 0.85rem;
  }

  .list-row:hover {
    background: var(--bg-3);
  }

  .fake-map {
    position: relative;
    height: 16rem;
    background:
      repeating-linear-gradient(45deg, var(--bg-1) 0 12px, var(--bg-1) 12px 24px);
    border: 1px solid var(--border);
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: crosshair;
    overflow: hidden;
  }

  .map-hint {
    color: var(--text-2);
    font-size: 0.85rem;
    pointer-events: none;
  }

  .panel-chip {
    font-size: 0.78rem;
    color: var(--text-2);
  }

  .panel-body {
    margin: 0 0 0.75rem;
    font-size: 0.85rem;
  }

  .ov-controls {
    display: flex;
    flex-wrap: wrap;
    gap: 0.75rem 1.25rem;
    margin-bottom: 0.5rem;
  }

  .ov-controls label {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    font-size: 0.8rem;
    color: var(--text-2);
  }

  .ov-controls label.chk {
    gap: 0.3rem;
  }

  .ov-controls input[type="range"] {
    width: 9rem;
  }

  .ov-stage {
    position: relative;
    border: 1px solid var(--border);
    overflow: hidden;
  }

  .ov-base {
    position: absolute;
    inset: 0;
    display: block;
  }
</style>
