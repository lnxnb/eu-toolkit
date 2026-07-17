<!--
  IdentitySection — the country panel's Identity block (Sprint 1.2): country name
  + adjective (localized, loc overrides), map color, revolutionary colors (flag
  palette indices), flag replacement, and graphical culture. Every field pushes a
  composite onto the shared edit queue and reads its pending value back via the
  queue projection, so dirty/undo/save "just work".
-->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-dialog";
  import { SearchDropdown, ColorPicker } from "$lib/components/ui";
  import type { DropdownItem, RGB } from "$lib/components/ui";
  import type { EditQueue } from "$lib/edits.svelte";
  import FieldRow from "./FieldRow.svelte";
  import { scalarEdit, blockEdit } from "./fields";
  import type { CountryDetails, RegistryEntry } from "./types";

  let {
    installPath,
    modPath,
    tag,
    details,
    queue,
    oncolor,
  }: {
    installPath: string;
    modPath: string | null;
    tag: string;
    details: CountryDetails;
    queue: EditQueue;
    /** Report the pending map color (or null) so the map repaints live. */
    oncolor: (rgb: [number, number, number] | null) => void;
  } = $props();

  const cf = $derived(details.country_file ?? `common/countries/${details.name}.txt`);

  // --- Name + adjective (loc overrides) ---
  let pendingName = $derived(queue.pendingLocOverride(tag));
  let pendingAdj = $derived(queue.pendingLocOverride(`${tag}_ADJ`));
  let nameValue = $derived(pendingName ?? details.localized_name);
  let adjValue = $derived(pendingAdj ?? details.adjective ?? "");

  function commitName(next: string) {
    const v = next.trim();
    if (!v || v === (pendingName ?? details.localized_name)) return;
    queue.push({
      label: `Rename ${tag} to ${v}`,
      edits: [{ kind: "locOverride", key: tag, value: v }],
      coalesceKey: `name:${tag}`,
    });
  }
  function commitAdj(next: string) {
    const v = next.trim();
    if (v === (pendingAdj ?? details.adjective ?? "")) return;
    queue.push({
      label: `Set adjective of ${tag}`,
      edits: [{ kind: "locOverride", key: `${tag}_ADJ`, value: v }],
      coalesceKey: `adj:${tag}`,
    });
  }

  // --- Map color (setBlock on the country file) ---
  function parseTriple(s: string): [number, number, number] | null {
    const p = s.trim().split(/\s+/).map(Number);
    return p.length >= 3 && p.every((n) => Number.isFinite(n))
      ? [p[0], p[1], p[2]]
      : null;
  }
  let pendingColorStr = $derived(queue.pendingBlockValue(cf, ["color"]));
  let effectiveColor = $derived<[number, number, number] | null>(
    (pendingColorStr ? parseTriple(pendingColorStr) : null) ?? details.color,
  );
  let colorEdited = $derived(pendingColorStr !== undefined);
  let colorRGB = $derived<RGB>({
    r: effectiveColor?.[0] ?? 128,
    g: effectiveColor?.[1] ?? 128,
    b: effectiveColor?.[2] ?? 128,
  });

  // Push the pending color up to the map on every change (queue is source of
  // truth; this only drives the live compositor repaint).
  $effect(() => {
    oncolor(colorEdited ? effectiveColor : null);
  });

  function commitColor(c: RGB) {
    queue.push({
      label: `Set map color of ${tag}`,
      edits: [blockEdit(cf, "color", `${c.r} ${c.g} ${c.b}`)],
      coalesceKey: `color:${tag}`,
    });
  }

  // --- Revolutionary colors (flag-palette indices, NOT 0-255 RGB) ---
  // Present when the base file has them or an insert is already queued.
  let revoInsertPending = $derived(
    !!queue.findLast(
      (e) =>
        e.kind === "insertStatement" &&
        e.file === cf &&
        e.statement.trim().startsWith("revolutionary_colors"),
    ),
  );
  let revoBlockStr = $derived(queue.pendingBlockValue(cf, ["revolutionary_colors"]));
  let revoInsertStr = $derived.by(() => {
    const hit = queue.findLast(
      (e) =>
        e.kind === "insertStatement" &&
        e.file === cf &&
        e.statement.trim().startsWith("revolutionary_colors"),
    );
    if (hit?.kind !== "insertStatement") return undefined;
    const m = hit.statement.match(/\{([^}]*)\}/);
    return m ? m[1].trim() : undefined;
  });
  let revoPresent = $derived(details.revolutionary_colors != null || revoInsertPending);
  let revoValues = $derived<[number, number, number]>(
    (revoBlockStr ? parseTriple(revoBlockStr) : null) ??
      (revoInsertStr ? parseTriple(revoInsertStr) : null) ??
      details.revolutionary_colors ?? [0, 0, 0],
  );
  let revoEdited = $derived(revoBlockStr !== undefined || revoInsertPending);

  function commitRevo(idx: number, raw: string) {
    const n = Math.max(0, Math.min(64, Math.round(Number(raw) || 0)));
    const next: [number, number, number] = [revoValues[0], revoValues[1], revoValues[2]];
    next[idx] = n;
    const value = `${next[0]} ${next[1]} ${next[2]}`;
    // setBlock once the block exists (base or queued); else insert it whole.
    const edit = revoPresent
      ? blockEdit(cf, "revolutionary_colors", value)
      : ({
          kind: "insertStatement" as const,
          file: cf,
          blockPath: [],
          statement: `revolutionary_colors = { ${value} }`,
        });
    // Coalesce only when the base block already exists (a pure setBlock chain):
    // coalescing across an initial insert would drop the insert and break save.
    queue.push({
      label: `Set revolutionary colors of ${tag}`,
      edits: [edit],
      ...(details.revolutionary_colors != null ? { coalesceKey: `revo:${tag}` } : {}),
    });
  }

  // --- Flag replacement ---
  const flagFile = $derived(`gfx/flags/${tag}.tga`);
  // Preview URLs cached per flag file so undo/redo of a still-pending flag edit
  // keeps showing the replacement without re-converting.
  const previews = new Map<string, string>();
  let flagError = $state("");
  let converting = $state(false);
  let hasPendingFlag = $derived(
    !!queue.findLast((e) => e.kind === "binaryAsset" && e.file === flagFile),
  );
  let pendingFlagUrl = $derived(hasPendingFlag ? (previews.get(flagFile) ?? null) : null);

  async function replaceFlag() {
    flagError = "";
    const picked = await open({
      title: "Choose a flag image (PNG / JPG / BMP / TGA)",
      multiple: false,
      directory: false,
      filters: [{ name: "Image", extensions: ["png", "jpg", "jpeg", "bmp", "tga"] }],
    });
    if (typeof picked !== "string") return;
    converting = true;
    try {
      const res = await invoke<{ tga: number[]; preview: number[] }>("convert_flag", {
        path: picked,
      });
      const url = URL.createObjectURL(
        new Blob([new Uint8Array(res.preview)], { type: "image/png" }),
      );
      const prev = previews.get(flagFile);
      if (prev) URL.revokeObjectURL(prev);
      previews.set(flagFile, url);
      queue.push({
        label: `Replace flag of ${tag}`,
        edits: [{ kind: "binaryAsset", file: flagFile, bytes: res.tga }],
      });
    } catch (e) {
      flagError = String(e);
    }
    converting = false;
  }

  // --- Graphical culture (registry dropdown) ---
  let gfxCultures = $state<DropdownItem[]>([]);
  $effect(() => {
    invoke<RegistryEntry[]>("get_registry", { name: "graphical_cultures", installPath, modPath })
      .then((rows) => (gfxCultures = rows.map((r) => ({ key: r.key, label: r.name }))))
      .catch(() => (gfxCultures = []));
  });
  let pendingGfx = $derived(queue.pendingField(cf, "graphical_culture"));
  let gfxValue = $derived(
    pendingGfx !== undefined ? pendingGfx.value : (details.graphical_culture ?? null),
  );
  function commitGfx(key: string) {
    if (key === (details.graphical_culture ?? null)) return;
    queue.push({
      label: `Set graphical culture of ${tag}`,
      edits: [scalarEdit(cf, "graphical_culture", key, details.graphical_culture != null)],
    });
  }
</script>

<section>
  <h3>Identity</h3>

  <FieldRow label="Name" edited={pendingName !== undefined}>
    <input
      class="text"
      value={nameValue}
      onchange={(e) => commitName(e.currentTarget.value)}
    />
  </FieldRow>

  <FieldRow label="Adjective" edited={pendingAdj !== undefined}>
    <input
      class="text"
      value={adjValue}
      placeholder="(none)"
      onchange={(e) => commitAdj(e.currentTarget.value)}
    />
  </FieldRow>

  <FieldRow label="Map Color" edited={colorEdited}>
    <ColorPicker value={colorRGB} onchange={commitColor} />
    <span class="hex">rgb({colorRGB.r}, {colorRGB.g}, {colorRGB.b})</span>
  </FieldRow>

  <FieldRow label="Revolutionary Colors" edited={revoEdited}>
    <div class="revo">
      {#each [0, 1, 2] as i}
        <input
          class="num"
          type="number"
          min="0"
          max="64"
          value={revoValues[i]}
          onchange={(e) => commitRevo(i, e.currentTarget.value)}
        />
      {/each}
      <span class="hint" title="Indices into the game's flag color palette, not RGB">
        palette idx
      </span>
    </div>
  </FieldRow>

  <FieldRow label="Flag" edited={hasPendingFlag}>
    <div class="flag-row">
      {#if pendingFlagUrl}
        <img class="flag-preview" src={pendingFlagUrl} alt="Pending flag preview" />
      {/if}
      <button class="btn" onclick={replaceFlag} disabled={converting}>
        {converting ? "Converting…" : "Replace flag…"}
      </button>
    </div>
    {#if flagError}<span class="err">{flagError}</span>{/if}
  </FieldRow>

  <FieldRow label="Graphical Culture" edited={pendingGfx !== undefined}>
    <SearchDropdown
      items={gfxCultures}
      value={gfxValue}
      placeholder="Graphical culture…"
      onselect={commitGfx}
    />
  </FieldRow>
</section>

<style>
  section {
    margin-bottom: 1rem;
  }

  h3 {
    margin: 0 0 0.5rem;
    font-size: 0.8rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: #9ca3af;
  }

  .text,
  .num {
    background: #21262e;
    border: 1px solid #1f242c;
    color: #cfd4db;
    font-family: inherit;
    font-size: 0.85rem;
    padding: 0.25rem 0.4rem;
    outline: none;
  }

  .text {
    flex: 1;
    min-width: 0;
  }

  .num {
    width: 3.2rem;
  }

  .revo {
    display: flex;
    align-items: center;
    gap: 0.3rem;
  }

  .hint {
    font-size: 0.68rem;
    color: #8a919c;
  }

  .hex {
    font-size: 0.75rem;
    color: #8a919c;
    font-variant-numeric: tabular-nums;
  }

  .flag-row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .flag-preview {
    width: 2.5rem;
    height: 2.5rem;
    object-fit: cover;
    border: 1px solid #1f242c;
  }

  .btn {
    border: 1px solid #4b5563;
    background: transparent;
    color: inherit;
    font-family: inherit;
    font-size: 0.82rem;
    padding: 0.28rem 0.6rem;
    cursor: pointer;
  }

  .btn:hover:not(:disabled) {
    border-color: #9ca3af;
    background: #4a6da7;
    color: #fff;
  }

  .btn:disabled {
    opacity: 0.6;
    cursor: default;
  }

  .err {
    color: #fca5a5;
    font-size: 0.78rem;
  }
</style>
