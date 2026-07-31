<!--
  TradeNetworkOverlay — Sprint 8.1 network overlay for the Trade Nodes map mode.

  A transparent, pointer-events:none canvas layered over the map (like IconOverlay)
  that draws, glued to MapView's live viewport:
    • a marker at each node's location (distinct style for `end` nodes, a badge for
      `inland` nodes),
    • flow arrows along every route (Catmull-Rom through the route's own control
      points, arrowhead at the target end),
    • node-name labels past a zoom threshold (reuses overlay.ts fade helpers),
    • emphasis for the hovered/selected node and the hovered/selected route, and
    • draggable square handles for the route under edit (8.4).

  It owns only rendering — MapView does all hit-testing (markers/arrows/handles)
  and pointer handling, and passes the current selection + the live edit control
  points down. Repaints via a $effect on any input change, so it stays pixel-
  aligned through pan/zoom/edits with no second animation loop.
-->
<script lang="ts">
  import { project, iconOpacity, DEFAULT_CONFIG, type Viewport, type Point } from "$lib/overlay";
  import {
    sampleRouteCurve,
    markerPoint,
    type TradeNetwork,
    type Xy,
    type RouteRef,
  } from "$lib/tradenet";

  interface Props {
    network: TradeNetwork | null;
    centroids: Map<number, Point>;
    view: Viewport;
    cssWidth: number;
    cssHeight: number;
    selectedNode: string | null;
    hoverNode: string | null;
    selectedRoute: RouteRef | null;
    hoverRoute: RouteRef | null;
    /** Live control points (top-left) of the route under edit; overrides stored. */
    editControl: Xy[] | null;
    showUnassigned?: boolean;
  }

  let {
    network,
    centroids,
    view,
    cssWidth,
    cssHeight,
    selectedNode,
    hoverNode,
    selectedRoute,
    hoverRoute,
    editControl,
    showUnassigned = false,
  }: Props = $props();

  let canvas: HTMLCanvasElement;

  const HANDLE_PX = 9;

  function sameRoute(a: RouteRef | null, b: RouteRef | null): boolean {
    return !!a && !!b && a.from === b.from && a.index === b.index;
  }

  function nodeColor(c: [number, number, number] | null): string {
    // Canvas network colors encode game/map state and deliberately remain literals.
    return c ? `rgb(${c[0]}, ${c[1]}, ${c[2]})` : "#c9ccd1";
  }

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
    if (!network) return;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);

    drawRoutes(ctx);
    drawMarkers(ctx);
  }

  // --- Routes (arrows) -------------------------------------------------------

  function drawRoutes(ctx: CanvasRenderingContext2D) {
    if (!network) return;
    for (const node of network.nodes) {
      for (const route of node.outgoing) {
        const editing =
          editControl && selectedRoute && sameRoute(selectedRoute, { from: node.key, index: route.index });
        const control = editing ? editControl! : route.control;
        if (control.length < 2) continue;
        const emphasized =
          sameRoute(selectedRoute, { from: node.key, index: route.index }) ||
          sameRoute(hoverRoute, { from: node.key, index: route.index });
        drawArrow(ctx, control, emphasized, !!editing);
      }
    }
  }

  function drawArrow(ctx: CanvasRenderingContext2D, control: Xy[], emphasized: boolean, editing: boolean) {
    // Wrap-aware sampling: a route crossing the antimeridian comes back as
    // several on-map pieces (exit one edge, re-enter the other).
    const pieces = sampleRouteCurve(control, network!.map_width, 16)
      .filter((p) => p.length >= 2)
      .map((piece) => piece.map((p) => project({ x: p[0], y: p[1] }, view)));
    if (pieces.length === 0) return;
    ctx.lineJoin = "round";
    ctx.lineCap = "round";

    // Halo for legibility over any map color.
    ctx.strokeStyle = "rgba(10, 12, 15, 0.55)";
    ctx.lineWidth = emphasized ? 5 : 3;
    for (const piece of pieces) strokeCurve(ctx, piece);

    ctx.strokeStyle = emphasized ? "#ffd873" : "rgba(240, 244, 250, 0.9)";
    ctx.lineWidth = emphasized ? 2.6 : 1.5;
    for (const piece of pieces) strokeCurve(ctx, piece);

    // Arrowhead at the target end (last segment direction of the last piece).
    const curve = pieces[pieces.length - 1];
    const n = curve.length;
    if (n >= 2) {
      const tip = curve[n - 1];
      let ref = curve[n - 2];
      // Skip zero-length tail so the head points sensibly.
      for (let i = n - 2; i > 0 && ref.x === tip.x && ref.y === tip.y; i--) ref = curve[i];
      drawHead(ctx, ref, tip, emphasized ? "#ffd873" : "rgba(240, 244, 250, 0.95)", emphasized ? 12 : 9);
    }

    if (editing) {
      for (const p of control) {
        const s = project({ x: p[0], y: p[1] }, view);
        drawHandle(ctx, s.x, s.y);
      }
    }
  }

  function strokeCurve(ctx: CanvasRenderingContext2D, pts: Point[]) {
    ctx.beginPath();
    ctx.moveTo(pts[0].x, pts[0].y);
    for (let i = 1; i < pts.length; i++) ctx.lineTo(pts[i].x, pts[i].y);
    ctx.stroke();
  }

  function drawHead(ctx: CanvasRenderingContext2D, from: Point, to: Point, color: string, size: number) {
    const ang = Math.atan2(to.y - from.y, to.x - from.x);
    const a = Math.PI / 7;
    ctx.fillStyle = color;
    ctx.beginPath();
    ctx.moveTo(to.x, to.y);
    ctx.lineTo(to.x - size * Math.cos(ang - a), to.y - size * Math.sin(ang - a));
    ctx.lineTo(to.x - size * Math.cos(ang + a), to.y - size * Math.sin(ang + a));
    ctx.closePath();
    ctx.fill();
  }

  function drawHandle(ctx: CanvasRenderingContext2D, x: number, y: number) {
    const h = HANDLE_PX;
    ctx.fillStyle = "#ffffff";
    ctx.strokeStyle = "#1b3b6f";
    ctx.lineWidth = 1.5;
    ctx.fillRect(x - h / 2, y - h / 2, h, h);
    ctx.strokeRect(x - h / 2, y - h / 2, h, h);
  }

  // --- Markers ---------------------------------------------------------------

  function drawMarkers(ctx: CanvasRenderingContext2D) {
    if (!network) return;
    const labelOpacity = iconOpacity(view.scale, DEFAULT_CONFIG);
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";

    for (const node of network.nodes) {
      const m = markerPoint(node, centroids);
      if (!m) continue;
      const s = project(m, view);
      if (s.x < -40 || s.y < -40 || s.x > cssWidth + 40 || s.y > cssHeight + 40) continue;

      const isSel = node.key === selectedNode;
      const isHover = node.key === hoverNode;
      const r = node.end ? 8 : 6;
      const fill = nodeColor(node.color);

      ctx.lineWidth = isSel ? 3 : 2;
      ctx.strokeStyle = isSel ? "#ffd873" : isHover ? "#ffffff" : "#10141a";

      if (node.end) {
        // End node: diamond with a double ring.
        ctx.fillStyle = fill;
        diamond(ctx, s.x, s.y, r + 1);
        ctx.fill();
        ctx.stroke();
        ctx.beginPath();
        ctx.arc(s.x, s.y, r + 4, 0, Math.PI * 2);
        ctx.strokeStyle = isSel ? "#ffd873" : "rgba(16, 20, 26, 0.85)";
        ctx.lineWidth = 1.5;
        ctx.stroke();
      } else {
        ctx.beginPath();
        ctx.arc(s.x, s.y, r, 0, Math.PI * 2);
        ctx.fillStyle = fill;
        ctx.fill();
        ctx.stroke();
      }

      // Inland badge: a small square tag on the upper-right of the marker.
      if (node.inland) {
        const bx = s.x + r + 2;
        const by = s.y - r - 2;
        ctx.fillStyle = "#8b5a2b";
        ctx.strokeStyle = "#10141a";
        ctx.lineWidth = 1;
        ctx.fillRect(bx - 5, by - 5, 10, 10);
        ctx.strokeRect(bx - 5, by - 5, 10, 10);
        ctx.fillStyle = "#fff";
        ctx.font = "700 8px Inter, system-ui, sans-serif";
        ctx.fillText("I", bx, by + 0.5);
      }

      // Name label past the zoom threshold, or always for the selected node.
      const op = isSel ? Math.max(labelOpacity, 1) : labelOpacity;
      if (op > 0.02) {
        drawLabel(ctx, node.name, s.x, s.y + r + 9, op, isSel);
      }
    }
  }

  function diamond(ctx: CanvasRenderingContext2D, x: number, y: number, r: number) {
    ctx.beginPath();
    ctx.moveTo(x, y - r);
    ctx.lineTo(x + r, y);
    ctx.lineTo(x, y + r);
    ctx.lineTo(x - r, y);
    ctx.closePath();
  }

  function drawLabel(
    ctx: CanvasRenderingContext2D,
    text: string,
    x: number,
    y: number,
    opacity: number,
    sel: boolean,
  ) {
    const fontPx = sel ? 12 : 11;
    ctx.font = `${sel ? 700 : 600} ${fontPx}px Inter, system-ui, sans-serif`;
    const w = ctx.measureText(text).width;
    const padX = 4;
    const padY = 2;
    const boxW = w + padX * 2;
    const boxH = fontPx + padY * 2;
    ctx.globalAlpha = opacity;
    ctx.fillStyle = sel ? "rgba(40, 30, 8, 0.86)" : "rgba(20, 24, 29, 0.78)";
    ctx.fillRect(x - boxW / 2, y - boxH / 2, boxW, boxH);
    ctx.fillStyle = sel ? "#ffe6a3" : "#eef1f5";
    ctx.fillText(text, x, y + 0.5);
    ctx.globalAlpha = 1;
  }

  $effect(() => {
    // Touch reactive inputs so the effect re-runs on any change.
    void view.scale;
    void view.offsetX;
    void view.offsetY;
    void network;
    void centroids;
    void selectedNode;
    void hoverNode;
    void selectedRoute;
    void hoverRoute;
    void editControl;
    void showUnassigned;
    void cssWidth;
    void cssHeight;
    draw();
  });
</script>

<canvas
  bind:this={canvas}
  class="trade-net-overlay"
  style="width:{cssWidth}px; height:{cssHeight}px;"
></canvas>

<style>
  .trade-net-overlay {
    position: absolute;
    inset: 0;
    pointer-events: none;
    /* Above the map canvas, below the toolbar (z-index 10) and panels. */
    z-index: 5;
  }
</style>
