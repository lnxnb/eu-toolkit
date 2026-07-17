// Sprint 7 — Trade Goods: frontend data contracts + pure helpers.
//
// Interfaces mirror the backend `tradegoods.rs` payload. NOTE the field casing:
// `TradeGood`/`TradeGoodsPayload`/`TradeGoodScaffold` carry `#[serde(rename_all =
// "camelCase")]`, but the nested `ChanceSummary`, `ModRow` and `RawEntry` structs
// do NOT — so their fields arrive snake_case (`base_factor`, …). The interfaces
// below match the wire exactly.
//
// The pure functions (percentage normalization, 0-255 ↔ 0-1 2dp color conversion,
// modifier summaries) are dependency-free so they unit-test in isolation
// (scratchpad checks) and are shared by the list/editor/probability components.

import type { Rgb } from "$lib/mapmode";

export type ModKind = "percent" | "flat" | "boolean" | "unknown";

/** One modifier key/value row with its typed input kind (from the backend). */
export interface ModRow {
  key: string;
  value: string;
  kind: ModKind;
}

/** An unmodeled statement in a good block, preserved read-only (advanced). */
export interface RawEntry {
  key: string;
  /** "scalar" or "block". */
  kind: string;
  value: string;
}

/** Base colonization weight + a summary of the conditional sub-blocks. */
export interface ChanceSummary {
  /** `chance.factor` as written, or null if there is no chance block. */
  base_factor: string | null;
  has_conditional_modifiers: boolean;
  conditional_count: number;
}

export interface TradeGood {
  key: string;
  index: number;
  localizedName: string;
  /** `color = { ... }` tokens exactly as written. */
  colorRaw: string[];
  colorIsFloat: boolean;
  /** 0-255 RGB for swatches / map painting. */
  rgb: [number, number, number] | null;
  basePrice: string | null;
  priceFile: string | null;
  modifierRows: ModRow[];
  provinceRows: ModRow[];
  chance: ChanceSummary;
  isLatent: boolean;
  isValuable: boolean;
  rawExtra: RawEntry[];
  sourceFile: string;
  /** Set by the frontend for a pending, not-yet-saved scaffold (no atlas frame). */
  pending?: boolean;
}

export interface TradeGoodsPayload {
  goods: TradeGood[];
  total: number;
  latentCount: number;
  withPriceCount: number;
}

export interface TradeGoodScaffold {
  key: string;
  index: number;
  rgb: [number, number, number];
  colorFloats: [string, string, string];
  // TypedEdit-shaped JSON values, fed straight into the pending queue.
  edits: unknown[];
}

/** The sentinel good key that means "no trade good" (uncolonized land). */
export const UNKNOWN_KEY = "unknown";

/**
 * The base good key of a trade_goods mode-data group. Undiscovered provinces
 * are grouped per spawn-distribution CLUSTER with keys like `unknown#3`
 * (backend goods_spawn); everything list- or paint-shaped works on the base
 * good key, while the cluster group drives hover/selection granularity.
 * Real good keys never contain `#` (Clausewitz identifiers).
 */
export function goodKeyOfGroup(groupKey: string): string {
  const i = groupKey.indexOf("#");
  return i < 0 ? groupKey : groupKey.slice(0, i);
}

// --- pure helpers ----------------------------------------------------------

/** Parses a color token list ("0.96" floats or "245" ints) into 0-255 RGB. */
export function colorTokensToRgb(tokens: string[]): Rgb | null {
  if (tokens.length < 3) return null;
  const nums = tokens.slice(0, 3).map(Number);
  if (nums.some((n) => !Number.isFinite(n))) return null;
  const isFloat = tokens.some((t) => t.includes("."));
  const to255 = (n: number) =>
    Math.max(0, Math.min(255, Math.round(isFloat ? n * 255 : n)));
  return [to255(nums[0]), to255(nums[1]), to255(nums[2])];
}

/**
 * 0-255 RGB → the good-color convention: three 0-1 floats each fixed to 2 dp
 * (matches the backend `floats_of`). Round-trips stably through
 * `colorFloatsToRgb` for the values the picker produces.
 */
export function rgbToColorFloats(rgb: Rgb): [string, string, string] {
  const f = (n: number) => (Math.max(0, Math.min(255, Math.round(n))) / 255).toFixed(2);
  return [f(rgb[0]), f(rgb[1]), f(rgb[2])];
}

/** The 0-1 float color string ("0.50 0.50 0.50") → 0-255 RGB. */
export function colorFloatsToRgb(floats: string[]): Rgb | null {
  if (floats.length < 3) return null;
  const nums = floats.slice(0, 3).map(Number);
  if (nums.some((n) => !Number.isFinite(n))) return null;
  const c = (n: number) => Math.max(0, Math.min(255, Math.round(n * 255)));
  return [c(nums[0]), c(nums[1]), c(nums[2])];
}

/**
 * Normalize raw colonization factors to percentages summing to exactly 100.
 * An all-zero (or empty) input splits evenly. Uses a largest-remainder-free
 * proportional split kept as floats (the probability editor keeps floats live;
 * the backend rewrites factors = percentages directly).
 */
export function factorsToPercentages(factors: number[]): number[] {
  const n = factors.length;
  if (n === 0) return [];
  const sum = factors.reduce((a, b) => a + b, 0);
  if (sum <= 0) return factors.map(() => 100 / n);
  return factors.map((f) => (f / sum) * 100);
}

/** Prettifies a raw key ("land_forcelimit_modifier" → "Land forcelimit modifier"). */
export function prettifyKey(key: string): string {
  const s = key.replace(/_/g, " ").trim();
  return s.charAt(0).toUpperCase() + s.slice(1);
}

/** A short human value for a modifier row, sign-prefixed. */
export function formatModifierValue(kind: ModKind, value: string): string {
  if (kind === "boolean") return value === "yes" ? "Yes" : "No";
  const n = Number(value);
  if (!Number.isFinite(n)) return value;
  if (kind === "percent") {
    const pct = Math.round(n * 1000) / 10;
    return `${pct > 0 ? "+" : ""}${pct}%`;
  }
  return `${n > 0 ? "+" : ""}${n}`;
}

/** A compact one-line summary of a set of modifier rows for the list subrow. */
export function modifierSummary(
  rows: ModRow[],
  labels: Map<string, string>,
  max = 3,
): string {
  if (rows.length === 0) return "";
  const parts = rows
    .slice(0, max)
    .map(
      (r) => `${formatModifierValue(r.kind, r.value)} ${labels.get(r.key) ?? prettifyKey(r.key)}`,
    );
  if (rows.length > max) parts.push(`+${rows.length - max} more`);
  return parts.join(", ");
}
