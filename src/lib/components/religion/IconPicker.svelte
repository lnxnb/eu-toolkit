<script lang="ts">
  // Religion icon picker: loads the religion sprite strip (get_icon_atlas
  // "religions") and shows the current frame + a pop-out grid of all frames.
  // Frame index = icon - 1; picking frame i writes icon = i + 1 (handled by parent).
  import { invoke } from "@tauri-apps/api/core";

  let {
    installPath,
    modPath,
    current,
    onpick,
  }: {
    installPath: string;
    modPath: string | null;
    /** Current frame index (icon - 1), or null. */
    current: number | null;
    onpick: (frame: number) => void;
  } = $props();

  let url = $state<string | null>(null);
  let frameW = $state(0);
  let frameH = $state(0);
  let count = $state(0);
  let open = $state(false);

  $effect(() => {
    let revoked: string | null = null;
    invoke<ArrayBuffer>("get_icon_atlas", { installPath, modPath, kind: "religions" })
      .then((buf) => {
        const hlen = new Uint32Array(buf.slice(0, 4))[0];
        const header = JSON.parse(
          new TextDecoder().decode(new Uint8Array(buf, 4, hlen)),
        ) as { frameW: number; frameH: number; count: number };
        frameW = header.frameW;
        frameH = header.frameH;
        count = header.count;
        const png = buf.slice(4 + hlen);
        const u = URL.createObjectURL(new Blob([png], { type: "image/png" }));
        url = u;
        revoked = u;
      })
      .catch(() => {});
    return () => {
      if (revoked) URL.revokeObjectURL(revoked);
      url = null;
    };
  });

  // Cap on-screen frame size so the grid stays compact.
  let disp = $derived(Math.min(frameH || 24, 24));
  let scale = $derived(frameH > 0 ? disp / frameH : 1);

  function frameStyle(i: number, size: number): string {
    if (!url) return "";
    const sw = frameW * scale;
    const sh = frameH * scale;
    return (
      `width:${sw}px;height:${sh}px;` +
      `background-image:url(${url});` +
      `background-position:${-i * sw}px 0;` +
      `background-size:${frameW * count * scale}px ${sh}px;` +
      `background-repeat:no-repeat;`
    );
  }

  function pick(i: number) {
    onpick(i);
    open = false;
  }
</script>

<div class="icon-picker">
  <button
    class="current"
    title={current != null ? `Icon ${current + 1}` : "No icon"}
    onclick={() => (open = !open)}
  >
    {#if url && current != null && current >= 0}
      <span class="frame" style={frameStyle(current, disp)}></span>
    {:else}
      <span class="none">?</span>
    {/if}
    <span class="caret">▾</span>
  </button>

  {#if open && url}
    <div class="grid">
      {#each Array(count) as _, i (i)}
        <button
          class="cell"
          class:sel={i === current}
          title={`Icon ${i + 1}`}
          onclick={() => pick(i)}
        >
          <span class="frame" style={frameStyle(i, disp)}></span>
        </button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .icon-picker {
    position: relative;
    display: inline-block;
  }
  .current {
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
    background: #14181d;
    border: 1px solid #4b5563;
    padding: 0.2rem 0.4rem;
    cursor: pointer;
  }
  .frame {
    display: inline-block;
    image-rendering: pixelated;
  }
  .none {
    color: #9ca3af;
    width: 1.2rem;
    text-align: center;
  }
  .caret {
    color: #9ca3af;
    font-size: 0.7rem;
  }
  .grid {
    position: absolute;
    top: 100%;
    left: 0;
    z-index: 20;
    margin-top: 2px;
    display: grid;
    grid-template-columns: repeat(8, auto);
    gap: 2px;
    padding: 4px;
    max-height: 12rem;
    overflow-y: auto;
    background: #3f4855;
    border: 1px solid #2b323d;
    box-shadow: 2px 3px 8px rgba(0, 0, 0, 0.35);
  }
  .cell {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 2px;
    background: #14181d;
    border: 1px solid transparent;
    cursor: pointer;
  }
  .cell:hover {
    border-color: #4a6da7;
  }
  .cell.sel {
    border-color: #86efac;
  }
</style>
