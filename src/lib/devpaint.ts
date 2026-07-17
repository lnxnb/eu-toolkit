// Development painting math (Sprint 9.1) — pure, framework-free, unit-testable.
//
// The dev airbrush accrues development continuously while the button is held
// (~2 dev/second per province under the brush). Because dev values in the files
// are integers, we keep a fractional accumulator per province per stroke and
// commit whole points as they accrue, distributing each committed point across
// the three components (base_tax / base_production / base_manpower) by the
// current 9.2 mix using an *incremental largest-remainder* rule so the mix stays
// honest over the whole stroke (not just per-point rounding drift).
//
// Floor rule: each component floors at 1 and never goes below; when every
// component is at the floor, lowering stops. Missing dev keys are treated as a
// base of 0 and created at the floor on their first committed raise point
// (0 + 1 = 1 → insert `base_tax = 1`).
//
// Component order is fixed to match the backend + dev icon atlas:
//   0 = base_tax, 1 = base_production, 2 = base_manpower.

/** Airbrush rate: development points per second per province. */
export const DEV_RATE = 2;
/** Every component floors at 1 and never goes below. */
export const DEV_FLOOR = 1;

/** Normalized (sum-to-1) split across [tax, production, manpower]. */
export type DevMix = [number, number, number];

/** The three history-file keys, in component order. */
export const DEV_KEYS = ["base_tax", "base_production", "base_manpower"] as const;
export type DevKey = (typeof DEV_KEYS)[number];

/** Paint direction: raise (+1) or lower (−1). One stroke is single-direction. */
export type DevDir = 1 | -1;

/**
 * Per-province stroke accumulator. `base`/`present` are the effective starting
 * component values (queue-folded base; an absent component has base 0 and
 * present=false). `alloc` counts whole points committed to each component this
 * stroke (magnitude, always ≥ 0); `points` is their sum; `carry` is the
 * sub-integer remainder awaiting the next whole point (discarded at stroke end).
 */
export interface DevAccum {
  base: [number, number, number];
  present: [boolean, boolean, boolean];
  alloc: [number, number, number];
  points: number;
  carry: number;
}

function clamp(v: number, lo: number, hi: number): number {
  return Math.min(hi, Math.max(lo, v));
}

/** A fresh accumulator over a province's effective (queue-folded) base state. */
export function newDevAccum(
  base: [number, number, number],
  present: [boolean, boolean, boolean],
): DevAccum {
  return {
    base: [base[0], base[1], base[2]],
    present: [present[0], present[1], present[2]],
    alloc: [0, 0, 0],
    points: 0,
    carry: 0,
  };
}

/** Current value of component `k` under direction `dir` (base ± committed points). */
export function devValue(a: DevAccum, k: number, dir: DevDir): number {
  return a.base[k] + dir * a.alloc[k];
}

/** Current total development (sum of the three component values). */
export function devTotal(a: DevAccum, dir: DevDir): number {
  return devValue(a, 0, dir) + devValue(a, 1, dir) + devValue(a, 2, dir);
}

/**
 * Can component `k` still take a point in direction `dir`?
 * Raise: always. Lower: only a present component still above the floor.
 */
function eligible(a: DevAccum, k: number, dir: DevDir): boolean {
  if (dir > 0) return true;
  return a.present[k] && a.base[k] - a.alloc[k] > DEV_FLOOR;
}

/**
 * Commit one whole point to the component that most deserves it under the mix,
 * via incremental largest-remainder: pick the eligible component maximizing
 * `mix[k]·M − alloc[k]` where `M` is the point count after this commit. Over a
 * run of M points this reproduces the largest-remainder apportionment of
 * `mix·M` exactly (when nothing is floored), keeping the mix honest.
 * Returns false when nothing is eligible (all components at the floor).
 */
export function commitOnePoint(a: DevAccum, mix: DevMix, dir: DevDir): boolean {
  const M = a.points + 1;
  let bestK = -1;
  let bestPriority = -Infinity;
  for (let k = 0; k < 3; k++) {
    if (!eligible(a, k, dir)) continue;
    const priority = mix[k] * M - a.alloc[k];
    if (priority > bestPriority) {
      bestPriority = priority;
      bestK = k;
    }
  }
  if (bestK < 0) return false;
  a.alloc[bestK] += 1;
  a.points = M;
  return true;
}

/**
 * Accrue `dtSeconds` of airbrushing at {@link DEV_RATE}, committing whole points
 * as they cross integer boundaries. Returns the number of points committed this
 * tick (0 when the accrual hasn't yet reached a whole point). If every component
 * is floored (lowering a fully-minimal province) the remaining carry is dropped
 * and accrual stops.
 */
export function tickDevAccum(
  a: DevAccum,
  dtSeconds: number,
  mix: DevMix,
  dir: DevDir,
): number {
  a.carry += DEV_RATE * Math.max(0, dtSeconds);
  let committed = 0;
  while (a.carry >= 1) {
    if (!commitOnePoint(a, mix, dir)) {
      a.carry = 0; // fully floored: discard the leftover, stop accruing
      break;
    }
    a.carry -= 1;
    committed++;
  }
  return committed;
}

/** One component's final edit at stroke end. */
export interface DevComponentEdit {
  /** Component index (0=tax, 1=production, 2=manpower). */
  index: number;
  /** History-file key. */
  key: DevKey;
  /** New integer value to write. */
  value: number;
  /** Whether the key already existed (setScalar) or must be inserted. */
  present: boolean;
}

/**
 * Translate a finished stroke accumulator into per-component edits — one entry
 * per component that actually received points. Leftover fractions are already
 * discarded (they never reached a whole point). Values are floored at 1.
 */
export function finalizeDevAccum(a: DevAccum, dir: DevDir): DevComponentEdit[] {
  const out: DevComponentEdit[] = [];
  for (let k = 0; k < 3; k++) {
    if (a.alloc[k] === 0) continue;
    const value = Math.max(DEV_FLOOR, devValue(a, k, dir));
    out.push({ index: k, key: DEV_KEYS[k], value, present: a.present[k] });
  }
  return out;
}

/**
 * Client-side mirror of the backend dev gradient ramp (map_renderer::dev_color):
 * pale green at dev 3 → dark green at dev 30. Lets pending paints recolor the
 * gradient live without a backend re-render, matching the rendered PNG exactly.
 */
export function devColor(total: number): [number, number, number] {
  const t = clamp((total - 3) / 27, 0, 1);
  const lerp = (lo: number, hi: number) => Math.round(lo + (hi - lo) * t);
  return [lerp(216, 20), lerp(232, 105), lerp(200, 20)];
}

// --- Session-persistent mix (like the brush size) ---------------------------

const MIX_KEY = "eu_toolkit_dev_mix";
const DEFAULT_MIX: DevMix = [1 / 3, 1 / 3, 1 / 3];

/** Loads the session's saved dev mix (default ⅓ each), re-normalized to sum 1. */
export function loadDevMix(): DevMix {
  try {
    const raw = sessionStorage.getItem(MIX_KEY);
    if (raw) {
      const arr = JSON.parse(raw);
      if (
        Array.isArray(arr) &&
        arr.length === 3 &&
        arr.every((n) => typeof n === "number" && Number.isFinite(n) && n >= 0)
      ) {
        const sum = arr[0] + arr[1] + arr[2];
        if (sum > 0) return [arr[0] / sum, arr[1] / sum, arr[2] / sum];
      }
    }
  } catch {
    /* sessionStorage / JSON unavailable — fall through to the default */
  }
  return [...DEFAULT_MIX] as DevMix;
}

/** Persists the dev mix for the rest of the session. */
export function saveDevMix(mix: DevMix): void {
  try {
    sessionStorage.setItem(MIX_KEY, JSON.stringify(mix));
  } catch {
    /* ignore */
  }
}
