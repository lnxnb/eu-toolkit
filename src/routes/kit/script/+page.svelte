<!--
  /kit/script — DEV BENCH for the Sprint 14.2 / 14.4 script-editor components.
  Not shipped UI (Sprints 15–17 wire these into the Decisions/Events/Missions
  screens). A separate sub-route (like kit/dynasty) so parallel agents extending
  the main bench don't collide here.

  Exercises, fully offline (no backend needed):
    • ScriptTreeEditor over a canned TreeNode fixture covering every row type —
      logical groups (AND/OR/NOT), a scope group (province id), typed leaves
      (tag/number/bool/comparison/string), a block-valued leaf and an unmodeled
      key (both raw), plus the raw/tree toggle. Emitted TypedEdit[] batches are
      rendered as JSON so the emission contract can be eyeballed. `validate` is a
      local brace/quote checker mirroring the backend, so raw editing works here.
    • SpritePicker over synthetic canvas-generated PNGs (incl. one that throws, to
      prove the error-placeholder path) with an injected loader — no session.
    • OverlaySurface — the shared full-screen shell, hosting a SpritePicker.
-->
<script lang="ts">
  import {
    ScriptTreeEditor,
    SpritePicker,
    OverlaySurface,
    type KnownKey,
    type ScriptBlock,
    type ScriptValidation,
  } from "$lib/components/script";
  import type { Sprite } from "$lib/components/script/SpritePicker.svelte";
  import type { DropdownItem } from "$lib/components/ui";
  import type { TypedEdit } from "$lib/edits.svelte";

  const FILE = "decisions/Demo.txt";
  const ROOT = ["demo", "block"];

  // --- Known-trigger registry subset (offline; real UI uses get_known_triggers) ---
  const KNOWN: KnownKey[] = [
    { key: "tag", argKind: "tag", displayName: "Is country" },
    { key: "is_year", argKind: "number", displayName: "Is year" },
    { key: "is_subject", argKind: "bool", displayName: "Is a subject" },
    { key: "num_of_cities", argKind: "comparison", displayName: "Number of provinces" },
    { key: "culture_group", argKind: "string", displayName: "Culture group is" },
    { key: "primary_culture", argKind: "string", displayName: "Primary culture is" },
    { key: "has_country_flag", argKind: "string", displayName: "Has country flag" },
  ];

  // Demonstration entity swatches and canvas ink are intentionally literal data colors.
  const COUNTRIES: DropdownItem[] = [
    { key: "FRA", label: "France", swatch: "#2b6cb0" },
    { key: "ENG", label: "England", swatch: "#c53030" },
    { key: "CAS", label: "Castile", swatch: "#d69e2e" },
    { key: "ROOT", label: "ROOT (self scope)", swatch: "#5a6470" },
  ];

  // --- Canned tree fixture (shape mirrors backend build_node output) ---
  const FIXTURE: ScriptBlock = {
    span: [0, 0],
    raw: `{
\tNOT = { has_country_flag = formed_france_flag }
\tOR = {
\t\tculture_group = french
\t\tprimary_culture = cosmopolitan_french
\t}
\ttag = FRA
\tis_year = 1500
\tis_subject = no
\tnum_of_cities = 3
\tnum_of_owned_provinces_with = { value = 10 culture_group = french }
\t183 = { add_permanent_claim = FRA }
\tsome_scripted_trigger = yes
}`,
    nodes: [
      {
        nodeType: "group",
        key: "NOT",
        path: [...ROOT, "NOT"],
        groupKind: "not",
        value: null,
        raw: "NOT = { has_country_flag = formed_france_flag }",
        children: [
          {
            nodeType: "leaf",
            key: "has_country_flag",
            path: [...ROOT, "NOT", "has_country_flag"],
            groupKind: "",
            value: { kind: "string", text: "formed_france_flag" },
            children: [],
            raw: "has_country_flag = formed_france_flag",
          },
        ],
      },
      {
        nodeType: "group",
        key: "OR",
        path: [...ROOT, "OR"],
        groupKind: "or",
        value: null,
        raw: "OR = { culture_group = french primary_culture = cosmopolitan_french }",
        children: [
          {
            nodeType: "leaf",
            key: "culture_group",
            path: [...ROOT, "OR", "culture_group"],
            groupKind: "",
            value: { kind: "string", text: "french" },
            children: [],
            raw: "culture_group = french",
          },
          {
            nodeType: "leaf",
            key: "primary_culture",
            path: [...ROOT, "OR", "primary_culture"],
            groupKind: "",
            value: { kind: "string", text: "cosmopolitan_french" },
            children: [],
            raw: "primary_culture = cosmopolitan_french",
          },
        ],
      },
      {
        nodeType: "leaf",
        key: "tag",
        path: [...ROOT, "tag"],
        groupKind: "",
        value: { kind: "tag", text: "FRA" },
        children: [],
        raw: "tag = FRA",
      },
      {
        nodeType: "leaf",
        key: "is_year",
        path: [...ROOT, "is_year"],
        groupKind: "",
        value: { kind: "number", text: "1500" },
        children: [],
        raw: "is_year = 1500",
      },
      {
        nodeType: "leaf",
        key: "is_subject",
        path: [...ROOT, "is_subject"],
        groupKind: "",
        value: { kind: "bool", text: "no" },
        children: [],
        raw: "is_subject = no",
      },
      {
        nodeType: "leaf",
        key: "num_of_cities",
        path: [...ROOT, "num_of_cities"],
        groupKind: "",
        value: { kind: "number", text: "3" },
        children: [],
        raw: "num_of_cities = 3",
      },
      {
        nodeType: "leaf",
        key: "num_of_owned_provinces_with",
        path: [...ROOT, "num_of_owned_provinces_with"],
        groupKind: "",
        value: { kind: "block", text: "{ value = 10 culture_group = french }" },
        children: [],
        raw: "num_of_owned_provinces_with = { value = 10 culture_group = french }",
      },
      {
        nodeType: "group",
        key: "183",
        path: [...ROOT, "183"],
        groupKind: "scope",
        value: null,
        raw: "183 = { add_permanent_claim = FRA }",
        children: [
          {
            nodeType: "leaf",
            key: "add_permanent_claim",
            path: [...ROOT, "183", "add_permanent_claim"],
            groupKind: "",
            value: { kind: "tag", text: "FRA" },
            children: [],
            raw: "add_permanent_claim = FRA",
          },
        ],
      },
      {
        nodeType: "leaf",
        key: "some_scripted_trigger",
        path: [...ROOT, "some_scripted_trigger"],
        groupKind: "",
        value: { kind: "bool", text: "yes" },
        children: [],
        raw: "some_scripted_trigger = yes",
      },
    ],
  };

  // --- Local validator mirroring the backend validate_fragment (offline) ---
  function localValidate(text: string): Promise<ScriptValidation> {
    let depth = 0;
    let i = 0;
    const src = text;
    while (i < src.length) {
      const c = src[i];
      if (c === "#") {
        while (i < src.length && src[i] !== "\n") i++;
      } else if (c === '"') {
        i++;
        while (i < src.length && src[i] !== '"') {
          if (src[i] === "\n") return Promise.resolve({ valid: false, error: "Unterminated string" });
          i++;
        }
        if (i >= src.length) return Promise.resolve({ valid: false, error: "Unterminated string" });
        i++;
      } else if (c === "{") {
        depth++;
        i++;
      } else if (c === "}") {
        depth--;
        if (depth < 0) return Promise.resolve({ valid: false, error: "Unmatched '}'" });
        i++;
      } else {
        i++;
      }
    }
    if (depth !== 0)
      return Promise.resolve({ valid: false, error: `${depth} unclosed '{' block(s)` });
    return Promise.resolve({ valid: true, error: null });
  }

  // --- Emitted-edit log ---
  let editLog = $state<{ label: string; edits: TypedEdit[] }[]>([]);
  function onedit(edits: TypedEdit[], label: string) {
    editLog = [{ label, edits }, ...editLog].slice(0, 30);
  }

  // --- Synthetic sprites for the SpritePicker (offline) ---
  const SPRITES: Sprite[] = [
    ...Array.from({ length: 24 }, (_, i) => ({
      name: `GFX_mission_demo_${String(i).padStart(2, "0")}`,
      texturefile: `gfx/interface/missions/demo_${i}.dds`,
    })),
    { name: "GFX_mission_unsupported_bc7", texturefile: "gfx/interface/missions/bc7.dds" },
  ];

  function makeSpritePng(seed: number): Promise<ArrayBuffer> {
    const c = document.createElement("canvas");
    c.width = 48;
    c.height = 48;
    const ctx = c.getContext("2d")!;
    const hue = (seed * 47) % 360;
    ctx.fillStyle = `hsl(${hue} 55% 30%)`;
    ctx.fillRect(0, 0, 48, 48);
    ctx.fillStyle = `hsl(${(hue + 40) % 360} 70% 60%)`;
    ctx.beginPath();
    ctx.arc(24, 24, 16, 0, Math.PI * 2);
    ctx.fill();
    ctx.fillStyle = "#fff";
    ctx.font = "bold 16px sans-serif";
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
    ctx.fillText(String(seed), 24, 25);
    return new Promise((resolve) =>
      c.toBlob((b) => b!.arrayBuffer().then(resolve), "image/png"),
    );
  }

  function loadSprite(name: string): Promise<ArrayBuffer> {
    if (name.includes("unsupported")) {
      return Promise.reject("unsupported DX10 DDS format (DXGI 98); BC7/other not decoded");
    }
    const m = /_(\d+)$/.exec(name);
    return makeSpritePng(m ? Number(m[1]) : 0);
  }

  let pickedSprite = $state<string | null>(null);

  // --- OverlaySurface demo ---
  let overlayOpen = $state(false);
  let overlaySprite = $state<string | null>(null);
</script>

<div class="bench">
  <header class="bench-head">
    <h1>Script Editor Bench</h1>
    <span class="note">Sprint 14.2 / 14.4 — dev-only, fully offline.</span>
  </header>

  <div class="grid">
    <section class="card wide">
      <h2>ScriptTreeEditor — conditions (canned fixture)</h2>
      <ScriptTreeEditor
        file={FILE}
        rootPath={ROOT}
        block={FIXTURE}
        registry="triggers"
        known={KNOWN}
        countries={COUNTRIES}
        validate={localValidate}
        {onedit}
      />
      <p class="out">
        The fixture is static — emitted edits are logged (not re-applied), so the tree
        won't visibly change; that's the point (props-in / edits-out).
      </p>
    </section>

    <section class="card wide">
      <h2>Emitted TypedEdit[] batches</h2>
      {#if editLog.length === 0}
        <p class="out">Interact with the editor above to see emitted edits.</p>
      {/if}
      <div class="log">
        {#each editLog as entry, i (i)}
          <div class="log-entry">
            <div class="log-label">{entry.label}</div>
            <pre>{JSON.stringify(entry.edits, null, 2)}</pre>
          </div>
        {/each}
      </div>
    </section>

    <section class="card wide">
      <h2>SpritePicker — synthetic sprites (lazy-loaded)</h2>
      <div class="sprite-host">
        <SpritePicker
          prefix="GFX_mission_"
          sprites={SPRITES}
          {loadSprite}
          bind:value={pickedSprite}
          onselect={(n) => (pickedSprite = n)}
        />
      </div>
      <p class="out">Selected: <code>{pickedSprite ?? "none"}</code></p>
    </section>

    <section class="card">
      <h2>OverlaySurface</h2>
      <button class="demo-btn" onclick={() => (overlayOpen = true)}>Open overlay…</button>
      <p class="out">Esc, the × button, or a left/right-click on the backdrop all close it.</p>
      <p class="out">Overlay-picked sprite: <code>{overlaySprite ?? "none"}</code></p>
    </section>
  </div>
</div>

<OverlaySurface bind:open={overlayOpen} title="Overlay Surface Demo">
  {#snippet toolbar()}
    <span class="ovl-toolbar-note">Sprint 15–17 host their editors in this shell</span>
  {/snippet}
  <div class="ovl-content">
    <p>Full-screen Windows-classic shell. Below is a SpritePicker inside it:</p>
    <div class="ovl-sprite-host">
      <SpritePicker
        prefix="GFX_mission_"
        sprites={SPRITES}
        {loadSprite}
        bind:value={overlaySprite}
        onselect={(n) => (overlaySprite = n)}
      />
    </div>
  </div>
</OverlaySurface>

<style>
  .bench {
    min-height: 100vh;
    background: var(--bg-0);
    color: var(--text-1);
    font-family: Inter, system-ui, sans-serif;
    padding: 1rem 1rem 4rem;
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

  code {
    color: var(--text-1);
    background: var(--bg-1);
    padding: 0 0.25rem;
  }

  .demo-btn {
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

  .log {
    max-height: 20rem;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
  }

  .log-entry {
    border: 1px solid var(--border);
    background: var(--bg-0);
  }

  .log-label {
    padding: 0.25rem 0.5rem;
    background: var(--bg-1);
    font-size: 0.78rem;
    color: var(--text-1);
  }

  .log-entry pre {
    margin: 0;
    padding: 0.4rem 0.5rem;
    font-size: 0.72rem;
    color: var(--ok);
    overflow-x: auto;
  }

  .sprite-host {
    height: 22rem;
  }

  .ovl-toolbar-note {
    font-size: 0.78rem;
    color: var(--text-1);
  }

  .ovl-content {
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
    height: 100%;
    min-height: 0;
  }

  .ovl-content p {
    margin: 0;
    font-size: 0.85rem;
  }

  .ovl-sprite-host {
    flex: 1;
    min-height: 0;
  }
</style>
