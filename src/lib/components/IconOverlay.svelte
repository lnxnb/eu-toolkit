<!--
  IconOverlay — Phase 0.7 reusable per-province overlay layer (spec 7.6).

  Draws an icon / text label / stat box centered on each province's centroid, on a
  canvas layered over the map. Consumed later by Trade Goods icons (7.6), Trade
  Node labels (8.1), Religion icons (5.x) and Dev stat boxes (9.3) — all the same
  machinery, differing only in the `items` map and `atlas` passed in.

  ── Integration contract (how MapView drops this in, Sprint 7) ────────────────
  Place it as a sibling of the map <canvas>, in the same stacking context, sized
  to the container (it is pointer-events:none, so map hit-testing is unaffected):

      <IconOverlay
        {provinceIds} {mapW} {mapH}          // the buffer MapView already loads
        view={{ scale, offsetX, offsetY }}   // MapView's live transform
        cssWidth={containerW} cssHeight={containerH}
        {items}                              // Map<provinceId, OverlayItem>
        {atlas}                              // { image, frameW, frameH, count } | null
        config={DEFAULT_CONFIG}
      />

  * `provinceIds` / `mapW` / `mapH` are exactly MapView's cached id buffer; the
    overlay computes centroids from it once and caches them (recompute only when
    the buffer identity changes).
  * `view` is MapView's `{ scale, offsetX, offsetY }`. Pass a NEW object on each
    pan/zoom (Svelte reactivity) — MapView already recomputes these in `redraw()`.
  * `items` maps province id → what to draw. For trade goods:
    `items.set(id, { iconIndex: atlasIndex[goodKey] })`, built from
    `get_mode_data` values + the atlas index map. Absent ids draw nothing
    (unknown/no-good provinces show nothing, per spec).
  * `atlas.image` is an ImageBitmap of the strip PNG from `get_icon_atlas`;
    `iconIndex` selects frame `iconIndex` (sliced at `x = iconIndex*frameW`).
  * The overlay owns only rendering — no input, no map state. It never re-renders
    the map; it repaints its own canvas whenever `view`/`items`/`atlas` change,
    so it stays glued to the map through pan/zoom/edits with no extra wiring.
-->
<script lang="ts">
  import {
    computeCentroids,
    project,
    iconOpacity,
    iconSize,
    isVisible,
    DEFAULT_CONFIG,
    type Viewport,
    type OverlayItem,
    type Atlas,
    type OverlayConfig,
    type Point,
  } from "$lib/overlay";

  interface Props {
    provinceIds: Uint16Array | null;
    mapW: number;
    mapH: number;
    view: Viewport;
    /** Overlay canvas CSS size (matches the map container). */
    cssWidth: number;
    cssHeight: number;
    items: Map<number, OverlayItem>;
    atlas?: Atlas | null;
    config?: OverlayConfig;
  }

  let {
    provinceIds,
    mapW,
    mapH,
    view,
    cssWidth,
    cssHeight,
    items,
    atlas = null,
    config = DEFAULT_CONFIG,
  }: Props = $props();

  let canvas: HTMLCanvasElement;

  // Centroids are expensive to compute but stable for a session: cache them and
  // only rebuild when the province-id buffer instance changes.
  let cacheSource: Uint16Array | null = null;
  let centroids: Map<number, Point> = new Map();

  function ensureCentroids() {
    if (!provinceIds) {
      centroids = new Map();
      cacheSource = null;
      return;
    }
    if (cacheSource === provinceIds) return;
    centroids = computeCentroids(provinceIds, mapW, mapH);
    cacheSource = provinceIds;
  }

  function draw() {
    if (!canvas) return;
    const dpr = window.devicePixelRatio || 1;
    // Size the backing store to device pixels; CSS size is set in the template.
    const bw = Math.max(1, Math.round(cssWidth * dpr));
    const bh = Math.max(1, Math.round(cssHeight * dpr));
    if (canvas.width !== bw) canvas.width = bw;
    if (canvas.height !== bh) canvas.height = bh;

    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.clearRect(0, 0, canvas.width, canvas.height);

    const opacity = iconOpacity(view.scale, config);
    if (opacity <= 0 || items.size === 0) return;
    ensureCentroids();
    if (centroids.size === 0) return;

    // Work in CSS pixels; scale by dpr for the backing store.
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.globalAlpha = opacity;

    const size = iconSize(view.scale, config);
    const half = size / 2;
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";

    for (const [id, item] of items) {
      const c = centroids.get(id);
      if (!c) continue;
      const s = project(c, view);
      if (!isVisible(s.x, s.y, half + 8, cssWidth, cssHeight)) continue;

      if (item.iconIndex !== undefined && atlas) {
        const fi = Math.max(0, Math.min(atlas.count - 1, item.iconIndex));
        ctx.drawImage(
          atlas.image,
          fi * atlas.frameW,
          0,
          atlas.frameW,
          atlas.frameH,
          s.x - half,
          s.y - half,
          size,
          size,
        );
        // Secondary badge (S3.3 trade modifier): smaller, offset to the top-right
        // so both glyphs read at the same anchor without overlapping.
        if (item.badgeIndex !== undefined) {
          const bi = Math.max(0, Math.min(atlas.count - 1, item.badgeIndex));
          const bsize = size * 0.6;
          const bx = s.x + half - bsize * 0.55;
          const by = s.y - half - bsize * 0.25;
          ctx.drawImage(
            atlas.image,
            bi * atlas.frameW,
            0,
            atlas.frameW,
            atlas.frameH,
            bx,
            by,
            bsize,
            bsize,
          );
        }
      }

      if (item.label) {
        drawLabel(ctx, item.label, s.x, s.y, size);
      }

      if (item.statBox) {
        drawStatBox(ctx, item.statBox, s.x, s.y, size, item.statIcons);
      }
    }
    ctx.globalAlpha = 1;
  }

  function drawLabel(
    ctx: CanvasRenderingContext2D,
    text: string,
    x: number,
    y: number,
    size: number,
  ) {
    const fontPx = Math.max(9, Math.min(16, size * 0.42));
    ctx.font = `${fontPx}px Inter, system-ui, sans-serif`;
    const w = ctx.measureText(text).width;
    const padX = 4;
    const padY = 2;
    const boxW = w + padX * 2;
    const boxH = fontPx + padY * 2;
    ctx.fillStyle = "rgba(20, 24, 29, 0.72)";
    ctx.fillRect(x - boxW / 2, y - boxH / 2, boxW, boxH);
    // Canvas glyph ink is part of the rendered map overlay, not DOM chrome.
    ctx.fillStyle = "#eef1f5";
    ctx.fillText(text, x, y + 0.5);
  }

  function drawStatBox(
    ctx: CanvasRenderingContext2D,
    lines: string[],
    x: number,
    y: number,
    size: number,
    statIcons?: number[],
  ) {
    const fontPx = Math.max(9, Math.min(14, size * 0.34));
    ctx.font = `${fontPx}px Inter, system-ui, sans-serif`;
    const lineH = fontPx + 3;
    // Icon glyph size + gap when a per-line atlas frame is supplied (9.3).
    const useIcons = !!(statIcons && atlas && statIcons.length === lines.length);
    const iconSz = useIcons ? Math.round(lineH * 0.9) : 0;
    const iconGap = useIcons ? 3 : 0;
    const w = Math.max(...lines.map((l) => ctx.measureText(l).width));
    const padX = 4;
    const padY = 3;
    const boxW = w + iconSz + iconGap + padX * 2;
    const boxH = lines.length * lineH + padY * 2 - 3;
    const top = y - boxH / 2;
    const left = x - boxW / 2;
    ctx.fillStyle = "rgba(20, 24, 29, 0.78)";
    ctx.fillRect(left, top, boxW, boxH);
    ctx.fillStyle = "#dfe4ea";
    // Text is left-aligned when icons share the row so numbers line up; the
    // whole block stays centered on the province via `left`.
    const prevAlign = ctx.textAlign;
    if (useIcons) ctx.textAlign = "left";
    for (let i = 0; i < lines.length; i++) {
      const cy = top + padY + i * lineH + lineH / 2 - 1;
      if (useIcons && atlas) {
        const fi = Math.max(0, Math.min(atlas.count - 1, statIcons![i]));
        ctx.drawImage(
          atlas.image,
          fi * atlas.frameW,
          0,
          atlas.frameW,
          atlas.frameH,
          left + padX,
          cy - iconSz / 2,
          iconSz,
          iconSz,
        );
        ctx.fillText(lines[i], left + padX + iconSz + iconGap, cy);
      } else {
        ctx.fillText(lines[i], x, cy);
      }
    }
    ctx.textAlign = prevAlign;
  }

  // Repaint whenever anything that affects the picture changes. Reading these in
  // the effect registers them as dependencies (Svelte 5 runes).
  $effect(() => {
    // Touch reactive inputs so the effect re-runs on any change.
    void view.scale;
    void view.offsetX;
    void view.offsetY;
    void items;
    void atlas;
    void cssWidth;
    void cssHeight;
    void provinceIds;
    draw();
  });
</script>

<canvas
  bind:this={canvas}
  class="icon-overlay"
  style="width:{cssWidth}px; height:{cssHeight}px;"
></canvas>

<style>
  .icon-overlay {
    position: absolute;
    inset: 0;
    pointer-events: none;
    /* Sits above the map canvas but below toolbar/panels (toolbar is z-index 10). */
    z-index: 5;
  }
</style>
