<!--
  IconImportButton (S2.5) — shared "import custom art" affordance for the religion
  and trade-good panels. Opens a file dialog, splices the picked image into the
  target strip's positional frame via the `import_icon` backend command, and
  queues the whole re-encoded strip as a pending BinaryAsset (copy-on-write; the
  base install is never touched). Shows the returned tile PNG as an immediate
  preview so the new art is visible before save — the icon overlay / list rows
  refresh from the reloaded atlas after save.

  Multiple imports before a save chain: the last pending strip bytes for this file
  (an earlier import or a create-good scaffold) are passed as `pendingStrip` so the
  splice composes on top and the last (superset) BinaryAsset wins.
-->
<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import { invoke } from "@tauri-apps/api/core";
  import type { EditQueue } from "$lib/edits.svelte";

  // trade_goods → resources.dds; religions → icon_religion.dds. Mirrors
  // `import_strip_rel` in the backend `icons.rs`.
  const STRIP_FILE: Record<string, string> = {
    trade_goods: "gfx/interface/resources.dds",
    religions: "gfx/interface/icon_religion.dds",
  };

  let {
    installPath,
    modPath,
    kind,
    frame,
    label = "Import art…",
    disabled = false,
    queue,
    size = 26,
  }: {
    installPath: string;
    modPath: string | null;
    /** "trade_goods" | "religions". */
    kind: string;
    /** Positional strip frame to replace (good index / icon − 1). < 0 disables. */
    frame: number;
    label?: string;
    disabled?: boolean;
    queue: EditQueue;
    size?: number;
  } = $props();

  const stripFile = $derived(STRIP_FILE[kind] ?? "");

  let busy = $state(false);
  let err = $state("");
  let previewUrl = $state<string | null>(null);

  interface ImportedIcon {
    file: string;
    strip: number[];
    tilePng: number[];
    frameW: number;
    frameH: number;
    frame: number;
  }

  // The most recent pending strip bytes for this file (chains multiple pre-save
  // imports / a create-good scaffold's extended strip).
  function pendingStripBytes(): number[] | null {
    const hit = queue.findLast((e) => e.kind === "binaryAsset" && e.file === stripFile);
    return hit?.kind === "binaryAsset" ? hit.bytes : null;
  }

  async function importArt() {
    if (frame < 0 || !stripFile) return;
    err = "";
    const picked = await open({
      title: "Choose icon art (PNG / JPG / TGA / DDS)",
      multiple: false,
      directory: false,
      filters: [{ name: "Image", extensions: ["png", "jpg", "jpeg", "bmp", "tga", "dds"] }],
    });
    if (typeof picked !== "string") return;
    busy = true;
    try {
      const res = await invoke<ImportedIcon>("import_icon", {
        installPath,
        modPath,
        kind,
        frame,
        sourcePath: picked,
        pendingStrip: pendingStripBytes(),
      });
      if (previewUrl) URL.revokeObjectURL(previewUrl);
      previewUrl = URL.createObjectURL(
        new Blob([new Uint8Array(res.tilePng)], { type: "image/png" }),
      );
      queue.push({
        label: `Import ${kind === "religions" ? "religion" : "trade good"} icon`,
        edits: [{ kind: "binaryAsset", file: res.file, bytes: res.strip }],
      });
    } catch (e) {
      err = String(e);
    }
    busy = false;
  }

  $effect(() => () => {
    if (previewUrl) URL.revokeObjectURL(previewUrl);
  });
</script>

<span class="icon-import">
  {#if previewUrl}
    <img
      class="preview"
      src={previewUrl}
      alt="Imported icon preview"
      style="width:{size}px;height:{size}px;"
    />
  {/if}
  <button class="btn" onclick={importArt} disabled={busy || disabled || frame < 0}>
    {busy ? "Importing…" : label}
  </button>
  {#if err}<span class="err" title={err}>!</span>{/if}
</span>

<style>
  .icon-import {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
  }
  .preview {
    display: block;
    flex: none;
    border: 1px solid var(--border);
    background: var(--bg-0);
    image-rendering: auto;
  }
  .btn {
    border: 1px solid var(--border-strong);
    background: transparent;
    color: var(--text-1);
    font-family: inherit;
    font-size: 0.8rem;
    padding: 0.2rem 0.6rem;
    cursor: pointer;
  }
  .btn:hover:not(:disabled) {
    border-color: var(--accent);
    background: var(--accent);
    color: var(--text-inverse);
  }
  .btn:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .err {
    color: var(--err);
    font-weight: 700;
    cursor: help;
  }
</style>
