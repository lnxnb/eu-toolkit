// Reusable map-brush infrastructure (Phase 0.5).
//
// Pure geometry + stroke sampling for the paint-style tools (add/remove
// province now; culture/religion/trade-good/dev painting later). The brush is
// deliberately tool-agnostic: it collects the set of province ids whose pixels
// fall under a circle (a single click, or a dragged stroke sampled so fast
// motion leaves no gaps), and the *tool* decides which of those ids are
// eligible and what edits they produce. Nothing here knows about ownership,
// religion, or the edit queue.
//
// The circle DIAMETER is defined in map pixels, so a brush is reproducible
// regardless of zoom/pan: at scale s a d-pixel brush is d*s screen pixels wide.

/** Smallest brush: a single map pixel (precise enough for a 1-pixel island). */
export const BRUSH_MIN = 1;
/** Largest brush: region-scale. */
export const BRUSH_MAX = 400;

const LOG_MIN = Math.log(BRUSH_MIN);
const LOG_MAX = Math.log(BRUSH_MAX);

function clamp(v: number, lo: number, hi: number): number {
  return Math.min(hi, Math.max(lo, v));
}

/** Slider position (0..1, logarithmic feel) → brush diameter in map pixels. */
export function sliderToSize(pos: number): number {
  return Math.round(Math.exp(LOG_MIN + (LOG_MAX - LOG_MIN) * clamp(pos, 0, 1)));
}

/** Brush diameter → slider position (0..1). Inverse of {@link sliderToSize}. */
export function sizeToSlider(size: number): number {
  const s = clamp(size, BRUSH_MIN, BRUSH_MAX);
  return (Math.log(s) - LOG_MIN) / (LOG_MAX - LOG_MIN);
}

/**
 * Photoshop-style `[` / `]` nudge: one logarithmic notch smaller/larger.
 * `dir` is -1 (smaller) or +1 (larger). Always changes by ≥ 1 map pixel.
 */
export function nudgeSize(size: number, dir: number): number {
  const next = sliderToSize(sizeToSlider(size) + dir * 0.04);
  if (next === size) return clamp(size + dir, BRUSH_MIN, BRUSH_MAX);
  return next;
}

// --- Session-persistent brush size ---------------------------------------

const SIZE_KEY = "eu_toolkit_brush_size";

/** Loads the session's saved brush size (default 12 map px). */
export function loadBrushSize(): number {
  try {
    const raw = sessionStorage.getItem(SIZE_KEY);
    if (raw) return clamp(parseInt(raw, 10) || 12, BRUSH_MIN, BRUSH_MAX);
  } catch {
    /* sessionStorage unavailable — fall through to the default */
  }
  return 12;
}

/** Persists the brush size for the rest of the session. */
export function saveBrushSize(size: number): void {
  try {
    sessionStorage.setItem(SIZE_KEY, String(Math.round(size)));
  } catch {
    /* ignore */
  }
}

// --- Circle collection ----------------------------------------------------

/** The "no province" sentinel in the province-id buffer. */
const NONE_ID = 0xffff;

/**
 * Adds to `out` every province id with ≥ 1 pixel inside the circle centered at
 * (`cx`, `cy`) map-pixel coordinates with the given `diameter` (map pixels).
 * Scans the circle's bounding box and filters to the radius. The sentinel id
 * (`0xffff`) and id 0 (unknown/off-map) are skipped. A ≤ 1 px diameter collects
 * the single pixel under the cursor, so one-pixel islands stay pickable.
 */
export function collectCircle(
  ids: Uint16Array,
  mapW: number,
  mapH: number,
  cx: number,
  cy: number,
  diameter: number,
  out: Set<number>,
): void {
  if (diameter <= 1) {
    const x = Math.floor(cx);
    const y = Math.floor(cy);
    if (x >= 0 && y >= 0 && x < mapW && y < mapH) {
      const id = ids[y * mapW + x];
      if (id !== NONE_ID && id !== 0) out.add(id);
    }
    return;
  }
  const r = diameter / 2;
  const r2 = r * r;
  const x0 = Math.max(0, Math.floor(cx - r));
  const x1 = Math.min(mapW - 1, Math.ceil(cx + r));
  const y0 = Math.max(0, Math.floor(cy - r));
  const y1 = Math.min(mapH - 1, Math.ceil(cy + r));
  for (let y = y0; y <= y1; y++) {
    const dy = y + 0.5 - cy;
    const row = y * mapW;
    for (let x = x0; x <= x1; x++) {
      const dx = x + 0.5 - cx;
      if (dx * dx + dy * dy > r2) continue;
      const id = ids[row + x];
      if (id !== NONE_ID && id !== 0) out.add(id);
    }
  }
}

// --- Continuous (airbrush) mode — Sprint 9.1 plumbing --------------------
//
// The default brush stamps each province at most once per stroke. Some tools
// (development airbrush, 9.1) instead want a value to *accrue* continuously
// while the button is held: on every frame, act on the provinces currently
// under the brush. This is the opt-in `continuous` surface — the API exists now
// so the brush contract is stable for its first consumer; the add/remove
// province tools do not use it.

/**
 * A continuous brush's per-frame callback: the province ids under the brush
 * right now, and the elapsed milliseconds since the previous tick (so a tool
 * can integrate a per-second rate, e.g. ~2 dev/second).
 */
export type ContinuousTick = (provinceIds: Set<number>, dtMs: number) => void;

/**
 * Drives `ontick` once per animation frame until the returned stop function is
 * called. `sample()` is invoked each frame to get the current province set
 * (the host recomputes it as the cursor moves). Not yet wired to a tool.
 */
export function runContinuous(
  sample: () => Set<number>,
  ontick: ContinuousTick,
): () => void {
  let raf = 0;
  let last = performance.now();
  const frame = (now: number) => {
    ontick(sample(), now - last);
    last = now;
    raf = requestAnimationFrame(frame);
  };
  raf = requestAnimationFrame(frame);
  return () => cancelAnimationFrame(raf);
}

/**
 * Sample centers along the segment from (`x0`,`y0`) to (`x1`,`y1`) so that
 * consecutive brush circles of `diameter` overlap (step = radius, ≥ 1 px). The
 * start point is excluded (already painted on the previous event); the end is
 * always included. Used so a fast drag paints a continuous ribbon.
 */
export function strokeSamples(
  x0: number,
  y0: number,
  x1: number,
  y1: number,
  diameter: number,
): Array<[number, number]> {
  const dx = x1 - x0;
  const dy = y1 - y0;
  const dist = Math.hypot(dx, dy);
  const step = Math.max(1, diameter / 2);
  const n = Math.max(1, Math.ceil(dist / step));
  const pts: Array<[number, number]> = [];
  for (let i = 1; i <= n; i++) {
    const t = i / n;
    pts.push([x0 + dx * t, y0 + dy * t]);
  }
  return pts;
}
