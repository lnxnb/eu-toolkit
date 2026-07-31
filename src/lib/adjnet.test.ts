import { describe, it, expect } from "vitest";
import {
  foldAdjacencies,
  rewriteEdit,
  adjLinePieces,
  adjSegments,
  trimSegment,
  dashForType,
  colorForType,
  adjacencyAt,
  suggestThrough,
  samePair,
  deriveType,
  baseToInputs,
  type AdjRow,
} from "./adjnet";
import type { Point } from "$lib/overlay";
import type { TypedEdit } from "$lib/edits.svelte";

function row(from: number, to: number, over: Partial<AdjRow> = {}): AdjRow {
  return {
    from,
    to,
    kind: "sea",
    through: 1300,
    startX: -1,
    startY: -1,
    stopX: -1,
    stopY: -1,
    comment: "",
    ...over,
  };
}

const MAPW = 5632;

describe("dash/color styles per type", () => {
  it("sea dashed, canal solid, land dotted, lake distinct", () => {
    expect(dashForType("sea").length).toBeGreaterThan(0);
    expect(dashForType("canal")).toEqual([]); // solid
    expect(dashForType("land").length).toBeGreaterThan(0);
    // lake is distinct from all the others (a dash-dot).
    const lake = dashForType("lake");
    expect(lake).not.toEqual(dashForType("sea"));
    expect(lake).not.toEqual(dashForType("land"));
    expect(lake.length).toBeGreaterThan(2);
  });
  it("each type has its own color", () => {
    const cols = ["sea", "canal", "land", "lake"].map(colorForType);
    expect(new Set(cols).size).toBe(4);
  });
});

describe("wrap-aware line geometry", () => {
  it("a normal in-bounds link is one straight piece", () => {
    const pieces = adjLinePieces([100, 200], [300, 250], MAPW);
    expect(pieces.length).toBe(1);
    expect(pieces[0][0]).toEqual([100, 200]);
    expect(pieces[0][pieces[0].length - 1]).toEqual([300, 250]);
  });

  it("an antimeridian-crossing link goes the SHORT way (two pieces at the edges)", () => {
    // Endpoints near opposite map edges: the short path wraps across the seam.
    const a: [number, number] = [50, 400];
    const b: [number, number] = [MAPW - 50, 400];
    const pieces = adjLinePieces(a, b, MAPW);
    expect(pieces.length).toBe(2);
    // Each piece stays within [0, mapW]; one exits at x=0, the other re-enters at x=mapW.
    for (const p of pieces) {
      for (const [x] of p) {
        expect(x).toBeGreaterThanOrEqual(-0.001);
        expect(x).toBeLessThanOrEqual(MAPW + 0.001);
      }
    }
    const xs = pieces.flat().map(([x]) => x);
    expect(Math.min(...xs)).toBeCloseTo(0, 3);
    expect(Math.max(...xs)).toBeCloseTo(MAPW, 3);
    // Total horizontal extent of the short way is ~100px, not ~mapW.
    const straightSpan = pieces
      .flatMap((p) => {
        let s = 0;
        for (let i = 1; i < p.length; i++) s += Math.abs(p[i][0] - p[i - 1][0]);
        return [s];
      })
      .reduce((a2, b2) => a2 + b2, 0);
    expect(straightSpan).toBeLessThan(200);
  });
});

describe("segment trimming (edge-to-edge crossings)", () => {
  // A 40px-wide, 1D world: province 1 at x<10, water 99 at 10..19, province 2
  // at x>=20 (any y).
  const W = 40;
  const idAt = (x: number) => (x < 10 ? 1 : x < 20 ? 99 : 2);

  it("trims a centroid-to-centroid line to the crossing gap", () => {
    const [a, b] = trimSegment([5, 0], [25, 0], 1, 2, idAt, W);
    // Exit of province 1 (last sample inside it) and entry into province 2.
    expect(a[0]).toBeGreaterThanOrEqual(9);
    expect(a[0]).toBeLessThan(10.5);
    expect(b[0]).toBeGreaterThanOrEqual(19.5);
    expect(b[0]).toBeLessThan(21);
  });

  it("falls back to the untrimmed endpoints when sampling never crosses", () => {
    const land = () => 7;
    expect(trimSegment([5, 0], [25, 0], 1, 2, land, W)).toEqual([
      [5, 0],
      [25, 0],
    ]);
  });

  it("trims across the antimeridian the short way", () => {
    // Province 1 at x<3, province 2 at x>=37, water between: the short way
    // crosses the seam, and the trimmed span is tiny (not ~mapW).
    const wrapIdAt = (x: number) => (x < 3 ? 1 : x >= 37 ? 2 : 99);
    const [a, b] = trimSegment([1, 0], [38, 0], 1, 2, wrapIdAt, W);
    const pieces = adjLinePieces(a, b, W);
    let span = 0;
    for (const p of pieces) {
      for (let i = 1; i < p.length; i++) span += Math.abs(p[i][0] - p[i - 1][0]);
    }
    expect(span).toBeLessThan(10);
  });

  it("adjSegments maps rows and passes untrimmed when no sampler", () => {
    const centroids = new Map<number, Point>([
      [1, { x: 5, y: 0 }],
      [2, { x: 25, y: 0 }],
    ]);
    const segs = adjSegments([row(1, 2), row(1, 999)], centroids, null, W);
    expect(segs[0]).toEqual([
      [5, 0],
      [25, 0],
    ]);
    expect(segs[1]).toBeNull();
    const trimmed = adjSegments([row(1, 2)], centroids, (x) => idAt(x), W);
    expect(trimmed[0]![0][0]).toBeGreaterThanOrEqual(9);
  });
});

describe("hit-testing", () => {
  const centroids = new Map<number, Point>([
    [1, { x: 100, y: 100 }],
    [2, { x: 300, y: 100 }],
    [3, { x: 100, y: 400 }],
  ]);
  const view = { scale: 1, offsetX: 0, offsetY: 0 };
  const segs = adjSegments([row(1, 2), row(1, 3)], centroids, null, MAPW);

  it("finds the line under the cursor", () => {
    // Midpoint of the 1↔2 horizontal line is (200,100).
    expect(adjacencyAt(segs, 200, 101, view, MAPW, 6)).toBe(0);
  });
  it("returns null when far from any line", () => {
    expect(adjacencyAt(segs, 250, 260, view, MAPW, 6)).toBeNull();
  });
  it("skips rows whose endpoint centroid is unknown", () => {
    const bad = adjSegments([row(1, 999)], centroids, null, MAPW);
    expect(adjacencyAt(bad, 200, 100, view, MAPW, 6)).toBeNull();
  });
});

describe("through suggestion", () => {
  const water = new Set([1300, 42]);
  it("uses the midpoint province when it is water", () => {
    const idAt = () => 1300;
    expect(suggestThrough([0, 0], [10, 0], water, idAt)).toBe(1300);
  });
  it("ring-searches outward to the nearest water tile", () => {
    // Midpoint (5,0) is land; a water tile sits a few px away.
    const idAt = (x: number, y: number) => (x === 5 && y === 3 ? 42 : 7);
    expect(suggestThrough([0, 0], [10, 0], water, idAt, 10)).toBe(42);
  });
  it("returns -1 when no water is within range", () => {
    const idAt = () => 7;
    expect(suggestThrough([0, 0], [10, 0], water, idAt, 5)).toBe(-1);
  });
});

describe("folds & helpers", () => {
  it("deriveType defaults to sea", () => {
    expect(deriveType(false, false)).toBe("sea");
  });
  it("samePair ignores direction", () => {
    expect(samePair(row(1, 2), row(2, 1))).toBe(true);
    expect(samePair(row(1, 2), row(1, 3))).toBe(false);
  });
  it("last csvRewrite wins in the fold", () => {
    const base = [row(1, 2), row(3, 4)];
    const first = rewriteEdit([
      { ...row(1, 2, { comment: "a" }), origin: 0 },
      { ...row(3, 4), origin: 1 },
    ]);
    const second = rewriteEdit([
      { ...row(1, 2, { comment: "b" }), origin: 0 },
      { ...row(3, 4), origin: 1 },
    ]);
    const edits: TypedEdit[] = [first, second];
    const folded = foldAdjacencies(base, edits);
    expect(folded[0].comment).toBe("b");
    expect(folded.length).toBe(2);
  });
  it("base with no edits maps to inputs with origin = index", () => {
    const inputs = baseToInputs([row(1, 2), row(3, 4)]);
    expect(inputs.map((r) => r.origin)).toEqual([0, 1]);
    expect(foldAdjacencies([row(1, 2)], []).length).toBe(1);
  });
});
