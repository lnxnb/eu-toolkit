// Shared map-mode selection/hover machinery (Phase 0.4).
//
// The backend `get_mode_data` command returns one payload that drives hover,
// selection, highlight and (for gradients) per-province values across every
// map mode. This module parses that payload and provides the client-side
// compositing pipeline: a pristine copy of the rendered mode image, plus
// per-province recolor overrides and a hover/selection darken overlay applied
// over it. Pending edits (Sprint 1+) feed the same override path so the map
// repaints live without a backend re-render.

/** The "no group / no value" sentinel — mirrors the province-id buffer's. */
export const NONE = 0xffff;

export type ModeKind = "categorical" | "gradient" | "raster";

export type Rgb = [number, number, number];

/** One selectable group in a categorical mode (country, religion, area, …). */
export interface ModeGroup {
  key: string;
  label: string;
  color: Rgb;
}

/**
 * One occupied province (political mode, Sprint 13.3): its owner color comes
 * from `values[id]`'s group; `color` is the controller's stripe color (rebel
 * gray for `controller = REB`). The compositor stripes owner/controller.
 */
export interface StripeEntry {
  id: number;
  color: Rgb;
}

export interface ModeData {
  kind: ModeKind;
  /** Categorical groups in stable order; empty for gradient/raster. */
  groups: ModeGroup[];
  /** Highest province id covered (`values.length === maxId + 1`). */
  maxId: number;
  /** Gradient decode factor: real value = `values[id] / valueScale`. */
  valueScale: number | null;
  /**
   * Province-id-indexed group index (categorical) or scaled value (gradient);
   * `NONE` where the province is outside the mode. Empty for raster.
   */
  values: Uint16Array;
  /**
   * Occupation stripes (political mode only; empty otherwise). Each names a
   * province occupied by a controller ≠ its owner, with the controller's stripe
   * color. Consumed by the occupation-render wave (Sprint 13B).
   */
  stripes: StripeEntry[];
}

/**
 * Decodes the `get_mode_data` wire buffer:
 * `[u32 headerLen][header JSON][u16 value per province id]`, little-endian.
 */
export function parseModeData(buf: ArrayBuffer): ModeData {
  const headerLen = new Uint32Array(buf.slice(0, 4))[0];
  const headerJson = new TextDecoder().decode(new Uint8Array(buf, 4, headerLen));
  const header = JSON.parse(headerJson) as {
    kind: ModeKind;
    groups?: ModeGroup[];
    maxId?: number;
    valueScale?: number | null;
    stripes?: StripeEntry[];
  };
  const valuesOffset = 4 + headerLen;
  // The offset may be odd (JSON length is arbitrary), so copy into an aligned
  // buffer rather than viewing — the values array is small (≤ ~130 KB).
  const values = new Uint16Array(buf.slice(valuesOffset));
  return {
    kind: header.kind,
    groups: header.groups ?? [],
    maxId: header.maxId ?? 0,
    valueScale: header.valueScale ?? null,
    values,
    stripes: header.stripes ?? [],
  };
}

/**
 * Land-border color drawn by the backend renderer (map_renderer::BORDER).
 * Border structure never changes when a province is recolored (it depends on
 * ids and water-ness, not fill colors), so the client-side recolor keeps these
 * pixels intact and re-derives water-water borders from the new fill.
 */
const LAND_BORDER: Rgb = [96, 96, 94];
/** Water-water border factor (map_renderer paint: fill × 0.86). */
const WATER_BORDER_FACTOR = 0.86;

/**
 * Diagonal-stripe band width for occupation rendering (Sprint 13.3): the band
 * `((x + y) / STRIPE_BAND) % 2` selects owner (0) vs controller (1) color.
 * MUST match the backend `map_renderer::STRIPE_BAND` so the client-side pending
 * repaint lines up pixel-for-pixel with the baked render.
 */
export const STRIPE_BAND = 8;

/**
 * Fixed stripe color for rebel-held provinces (`controller = REB`), mirroring
 * `map_renderer::REBEL_GRAY` so a pending rebel occupation stripes the same
 * gray the backend bakes.
 */
export const REBEL_GRAY: Rgb = [80, 80, 80];

/**
 * A recolor override: either a flat fill (`Rgb`), or an occupation stripe pair
 * `{ fill, stripe }` (owner fill / controller stripe, Sprint 13.3). The stripe
 * form paints diagonal bands matching the backend's `((x + y) / STRIPE_BAND)`.
 */
export type Override = Rgb | { fill: Rgb; stripe: Rgb };

/** True when an override is the owner/controller stripe form. */
function isStripe(o: Override): o is { fill: Rgb; stripe: Rgb } {
  return !Array.isArray(o);
}

/** The color an override paints at pixel index `i` (banded for stripe forms). */
function overrideColorAt(o: Override, i: number, width: number): Rgb {
  if (!isStripe(o)) return o;
  const x = i % width;
  const y = (i / width) | 0;
  return Math.floor((x + y) / STRIPE_BAND) % 2 === 0 ? o.fill : o.stripe;
}

/**
 * Stamps `overrides` (province id → {@link Override}) over the province's pixels
 * while preserving the renderer's border treatment: a border pixel (left/top
 * neighbor has a different id — exactly the backend's rule) that rendered as
 * the fixed land-border color stays untouched, and any other border pixel is
 * a water-water border, restamped as the (banded) fill × 0.86. Everything else
 * gets the flat (banded) fill. Stripe overrides paint diagonal owner/controller
 * bands matching the backend. Pure so it's unit-testable without a DOM.
 */
export function stampOverrides(
  dst: Uint8ClampedArray,
  pristine: Uint8ClampedArray,
  ids: Uint16Array,
  width: number,
  overrides: Map<number, Override>,
): void {
  for (let i = 0; i < ids.length; i++) {
    const o = overrides.get(ids[i]);
    if (!o) continue;
    const p = i * 4;
    const c = overrideColorAt(o, i, width);
    const isBorder =
      (i % width > 0 && ids[i - 1] !== ids[i]) ||
      (i >= width && ids[i - width] !== ids[i]);
    if (isBorder) {
      if (
        pristine[p] === LAND_BORDER[0] &&
        pristine[p + 1] === LAND_BORDER[1] &&
        pristine[p + 2] === LAND_BORDER[2]
      ) {
        continue; // land border: fixed color, independent of the fill
      }
      dst[p] = (c[0] * WATER_BORDER_FACTOR) | 0;
      dst[p + 1] = (c[1] * WATER_BORDER_FACTOR) | 0;
      dst[p + 2] = (c[2] * WATER_BORDER_FACTOR) | 0;
    } else {
      dst[p] = c[0];
      dst[p + 1] = c[1];
      dst[p + 2] = c[2];
    }
    // alpha stays 255 (pristine)
  }
}

/**
 * The client-side compositing pipeline for one rendered mode image. Holds a
 * pristine copy of the render, an offscreen "base" canvas (pristine + recolor
 * overrides) and a "highlight" overlay canvas (hover/selection darken). The
 * caller draws `base` then `overlay` to the screen — darken over override color
 * composites correctly when both touch the same province.
 */
export class MapCompositor {
  readonly width: number;
  readonly height: number;

  /** Pristine rendered pixels (RGBA); never mutated. */
  private readonly pristine: Uint8ClampedArray;
  private readonly ids: Uint16Array;

  private readonly baseCanvas: HTMLCanvasElement;
  private readonly baseData: ImageData;
  private readonly baseCtx: CanvasRenderingContext2D;

  private readonly overlayCanvas: HTMLCanvasElement;
  private readonly overlayData: ImageData;
  private readonly overlayCtx: CanvasRenderingContext2D;

  /**
   * @param pristine ImageData of the rendered mode (map resolution).
   * @param provinceIds Per-pixel province id buffer (length width*height).
   */
  constructor(pristine: ImageData, provinceIds: Uint16Array) {
    this.width = pristine.width;
    this.height = pristine.height;
    this.pristine = pristine.data;
    this.ids = provinceIds;

    this.baseCanvas = document.createElement("canvas");
    this.baseCanvas.width = this.width;
    this.baseCanvas.height = this.height;
    this.baseCtx = this.baseCanvas.getContext("2d")!;
    // Seed the base canvas with a private copy of the pristine pixels.
    this.baseData = new ImageData(
      new Uint8ClampedArray(pristine.data),
      this.width,
      this.height,
    );
    this.baseCtx.putImageData(this.baseData, 0, 0);

    this.overlayCanvas = document.createElement("canvas");
    this.overlayCanvas.width = this.width;
    this.overlayCanvas.height = this.height;
    this.overlayCtx = this.overlayCanvas.getContext("2d")!;
    this.overlayData = new ImageData(this.width, this.height);
  }

  /** The base image (pristine + recolor overrides) to draw first. */
  get base(): HTMLCanvasElement {
    return this.baseCanvas;
  }

  /** The hover/selection darken overlay to draw on top of the base. */
  get overlay(): HTMLCanvasElement {
    return this.overlayCanvas;
  }

  /**
   * Recolor by province id: repaint the base canvas as pristine everywhere,
   * then stamp `overrides` (province id -> RGB) over just those provinces'
   * pixels, preserving border shading (see stampOverrides). No backend
   * re-render — this is how pending edits repaint live.
   */
  setOverrides(overrides: Map<number, Override>): void {
    const dst = this.baseData.data;
    dst.set(this.pristine);
    if (overrides.size > 0) {
      stampOverrides(dst, this.pristine, this.ids, this.width, overrides);
    }
    this.baseCtx.putImageData(this.baseData, 0, 0);
  }

  /**
   * Bakes the current overridden base as the new pristine baseline. Used after
   * Save: the pending edits have been written to disk, so folding them into
   * pristine keeps the map showing the edited colors even though the edit queue
   * (and thus the recolor overrides) is about to be cleared — no backend
   * re-render needed.
   */
  commit(): void {
    this.pristine.set(this.baseData.data);
  }

  /**
   * Rebuild the darken overlay from a province-id-indexed table of packed
   * little-endian RGBA fills (0 = no darken). Single pass over the pixel id
   * buffer — the one highlight code path shared by hover and selection.
   */
  setHighlight(provinceFill: Uint32Array): void {
    const px = new Uint32Array(this.overlayData.data.buffer);
    const ids = this.ids;
    for (let i = 0; i < ids.length; i++) {
      px[i] = provinceFill[ids[i]];
    }
    this.overlayCtx.putImageData(this.overlayData, 0, 0);
  }
}
