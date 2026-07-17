<!--
  StripIcon — draws a single frame of a sprite strip (the trade-goods atlas from
  get_icon_atlas), or a client-side placeholder tile for a pending, not-yet-saved
  good (solid color + border + diagonal, styled like the backend's placeholder).

  Canvas-based so it reuses the overlay's already-decoded ImageBitmap (no second
  decode, no blob-URL lifecycle). Absent frame + no placeholder → nothing drawn.
-->
<script lang="ts">
  import type { Atlas } from "$lib/overlay";
  import type { Rgb } from "$lib/mapmode";

  let {
    atlas = null,
    frame = -1,
    size = 24,
    placeholder = null,
  }: {
    atlas?: Atlas | null;
    /** Frame index into the strip; < 0 or out of range draws the placeholder. */
    frame?: number;
    size?: number;
    /** Draw this RGB as a placeholder tile when no atlas frame is available. */
    placeholder?: Rgb | null;
  } = $props();

  let canvas: HTMLCanvasElement;

  function draw() {
    if (!canvas) return;
    const dpr = window.devicePixelRatio || 1;
    const bs = Math.max(1, Math.round(size * dpr));
    if (canvas.width !== bs) canvas.width = bs;
    if (canvas.height !== bs) canvas.height = bs;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, size, size);

    const hasFrame = atlas && frame >= 0 && frame < atlas.count;
    if (hasFrame) {
      ctx.imageSmoothingEnabled = true;
      ctx.drawImage(
        atlas!.image,
        frame * atlas!.frameW,
        0,
        atlas!.frameW,
        atlas!.frameH,
        0,
        0,
        size,
        size,
      );
      return;
    }
    if (placeholder) {
      const [r, g, b] = placeholder;
      ctx.fillStyle = `rgb(${r}, ${g}, ${b})`;
      ctx.fillRect(0, 0, size, size);
      // Diagonal + border so a placeholder reads as "not final art".
      ctx.strokeStyle = "rgba(0, 0, 0, 0.55)";
      ctx.lineWidth = 1;
      ctx.strokeRect(0.5, 0.5, size - 1, size - 1);
      ctx.beginPath();
      ctx.moveTo(1, 1);
      ctx.lineTo(size - 1, size - 1);
      ctx.stroke();
    }
  }

  $effect(() => {
    void atlas;
    void frame;
    void size;
    void placeholder;
    draw();
  });
</script>

<canvas
  bind:this={canvas}
  class="strip-icon"
  style="width:{size}px; height:{size}px;"
></canvas>

<style>
  .strip-icon {
    display: block;
    flex: none;
    image-rendering: auto;
  }
</style>
