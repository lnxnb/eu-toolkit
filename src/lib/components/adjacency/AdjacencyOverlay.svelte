<!--
  AdjacencyOverlay — Sprint 25 straits/adjacencies overlay for the Provinces map
  mode. A transparent, pointer-events:none canvas layered over the map (like
  TradeNetworkOverlay), drawing one line per adjacency between the endpoint
  provinces' centroids, styled by type (sea dashed, canal solid, land dotted,
  lake dash-dot). Wrap-aware (antimeridian links go the short way). The hovered
  and selected adjacency are emphasized, with endpoint markers on both.

  Rendering only — MapView owns hit-testing and pointer handling and passes the
  hovered/selected row index down. Repaints via a $effect on any input change.
-->
<script lang="ts">
  import { project, type Viewport, type Point } from "$lib/overlay";
  import {
    adjLinePieces,
    dashForType,
    colorForType,
    endpoints,
    type AdjRowInput,
  } from "$lib/adjnet";

  interface Props {
    rows: AdjRowInput[];
    centroids: Map<number, Point>;
    view: Viewport;
    cssWidth: number;
    cssHeight: number;
    mapW: number;
    hoverIndex: number | null;
    selectedIndex: number | null;
  }

  let {
    rows,
    centroids,
    view,
    cssWidth,
    cssHeight,
    mapW,
    hoverIndex,
    selectedIndex,
  }: Props = $props();

  let canvas: HTMLCanvasElement;

  function draw() {
    if (!canvas) return;
    const dpr = window.devicePixelRatio || 1;
    const bw = Math.max(1, Math.round(cssWidth * dpr));
    const bh = Math.max(1, Math.round(cssHeight * dpr));
    if (canvas.width !== bw) canvas.width = bw;
    if (canvas.height !== bh) canvas.height = bh;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.lineJoin = "round";
    ctx.lineCap = "butt";

    // Draw non-emphasized first, then the emphasized ones on top.
    for (let i = 0; i < rows.length; i++) {
      if (i === hoverIndex || i === selectedIndex) continue;
      drawLine(ctx, i, false);
    }
    for (let i = 0; i < rows.length; i++) {
      if (i === hoverIndex || i === selectedIndex) drawLine(ctx, i, true);
    }
  }

  function drawLine(ctx: CanvasRenderingContext2D, i: number, emph: boolean) {
    const r = rows[i];
    const ep = endpoints(r, centroids);
    if (!ep) return;
    const pieces = adjLinePieces(ep[0], ep[1], mapW).map((piece) =>
      piece.map((p) => project({ x: p[0], y: p[1] }, view)),
    );
    if (pieces.length === 0) return;

    // Dark halo for legibility over any map color.
    ctx.setLineDash([]);
    ctx.strokeStyle = "rgba(10, 12, 15, 0.6)";
    ctx.lineWidth = emph ? 5 : 3;
    for (const piece of pieces) stroke(ctx, piece);

    // Typed dashed/solid/dotted line.
    ctx.setLineDash(dashForType(r.kind));
    ctx.strokeStyle = emph ? "#ffe08a" : colorForType(r.kind);
    ctx.lineWidth = emph ? 2.8 : 1.7;
    for (const piece of pieces) stroke(ctx, piece);
    ctx.setLineDash([]);

    // Endpoint markers on the emphasized adjacency.
    if (emph) {
      for (const e of ep) {
        const s = project({ x: e[0], y: e[1] }, view);
        ctx.beginPath();
        ctx.arc(s.x, s.y, 5, 0, Math.PI * 2);
        ctx.fillStyle = "#ffe08a";
        ctx.strokeStyle = "#10141a";
        ctx.lineWidth = 1.5;
        ctx.fill();
        ctx.stroke();
      }
    }
  }

  function stroke(ctx: CanvasRenderingContext2D, pts: Point[]) {
    if (pts.length < 2) return;
    ctx.beginPath();
    ctx.moveTo(pts[0].x, pts[0].y);
    for (let i = 1; i < pts.length; i++) ctx.lineTo(pts[i].x, pts[i].y);
    ctx.stroke();
  }

  $effect(() => {
    void view.scale;
    void view.offsetX;
    void view.offsetY;
    void rows;
    void centroids;
    void hoverIndex;
    void selectedIndex;
    void cssWidth;
    void cssHeight;
    draw();
  });
</script>

<canvas
  bind:this={canvas}
  class="adjacency-overlay"
  style="width:{cssWidth}px; height:{cssHeight}px;"
></canvas>

<style>
  .adjacency-overlay {
    position: absolute;
    inset: 0;
    pointer-events: none;
    /* Above the map canvas, below the toolbar/panels (z-index 10). */
    z-index: 5;
  }
</style>
