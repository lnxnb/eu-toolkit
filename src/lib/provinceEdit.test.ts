import { describe, it, expect } from "vitest";
import {
  brushDisc,
  provincePixels,
  borderingProvinces,
  provinceColor,
  colorAt,
  paintOp,
  dissolveOp,
  applyOpsToRgba,
  NO_PROVINCE,
} from "./provinceEdit";

/** RGBA image data from an array of [r,g,b] pixels (alpha 255). */
function rgbaOf(pixels: [number, number, number][]): Uint8ClampedArray {
  const out = new Uint8ClampedArray(pixels.length * 4);
  pixels.forEach(([r, g, b], i) => {
    out[i * 4] = r;
    out[i * 4 + 1] = g;
    out[i * 4 + 2] = b;
    out[i * 4 + 3] = 255;
  });
  return out;
}
const rgbAt = (rgba: Uint8ClampedArray, i: number): [number, number, number] => [
  rgba[i * 4],
  rgba[i * 4 + 1],
  rgba[i * 4 + 2],
];

describe("brushDisc", () => {
  it("radius 0 paints the single center pixel", () => {
    expect(brushDisc(2, 1, 0, 5, 3)).toEqual([1 * 5 + 2]);
  });

  it("radius 1 paints a plus shape, clamped vertically", () => {
    // Center (2,0) on a 5x3 map: up is clamped off, so 4 pixels.
    const px = brushDisc(2, 0, 1, 5, 3).sort((a, b) => a - b);
    expect(px).toEqual([1, 2, 3, 7].sort((a, b) => a - b)); // (1,0)(2,0)(3,0)(2,1)
  });

  it("wraps horizontally at the antimeridian", () => {
    // Center at x=0 with radius 1 on width 5 -> left neighbour wraps to x=4.
    const px = brushDisc(0, 1, 1, 5, 3);
    expect(px).toContain(1 * 5 + 4); // wrapped left
    expect(px).toContain(1 * 5 + 0); // center
    expect(px).toContain(1 * 5 + 1); // right
  });
});

describe("provincePixels", () => {
  it("collects every pixel of a province", () => {
    // 3x2: ids [1,1,2, 2,3,1]
    const ids = new Uint16Array([1, 1, 2, 2, 3, 1]);
    expect(provincePixels(ids, 1).sort()).toEqual([0, 1, 5]);
    expect(provincePixels(ids, 2).sort()).toEqual([2, 3]);
    expect(provincePixels(ids, 3)).toEqual([4]);
  });
});

describe("borderingProvinces", () => {
  it("finds 4-adjacent neighbours, excluding self and sentinel", () => {
    // 3x3, province 5 is the center; neighbours are the 4 orthogonal cells.
    // ids:
    //   1 2 1
    //   3 5 4
    //   1 X 1     (X = sentinel)
    const ids = new Uint16Array([1, 2, 1, 3, 5, 4, 1, NO_PROVINCE, 1]);
    const nb = borderingProvinces(ids, 5, 3, 3).sort((a, b) => a - b);
    // Orthogonal to center idx4: up=2, left=3, right=4, down=sentinel(excluded)
    expect(nb).toEqual([2, 3, 4]);
  });

  it("honors horizontal wrap", () => {
    // 3x1: [7, 8, 9]; province 7 at x=0 wraps left to 9.
    const ids = new Uint16Array([7, 8, 9]);
    const nb = borderingProvinces(ids, 7, 3, 1).sort((a, b) => a - b);
    expect(nb).toEqual([8, 9]);
  });
});

describe("provinceColor / colorAt", () => {
  it("reads the bitmap color of a province from image data", () => {
    const ids = new Uint16Array([1, 2]);
    // RGBA for two pixels.
    const rgba = new Uint8ClampedArray([10, 20, 30, 255, 40, 50, 60, 255]);
    expect(provinceColor(ids, rgba, 1)).toEqual([10, 20, 30]);
    expect(provinceColor(ids, rgba, 2)).toEqual([40, 50, 60]);
    expect(provinceColor(ids, rgba, 99)).toBeNull();
    expect(colorAt(rgba, 1)).toEqual([40, 50, 60]);
  });
});

describe("applyOpsToRgba (client mirror of the backend engine)", () => {
  it("paint sets the listed pixels", () => {
    const rgba = rgbaOf([[1, 1, 1], [2, 2, 2], [3, 3, 3]]);
    applyOpsToRgba(rgba, [paintOp([1], [9, 8, 7])], 3, 1);
    expect(rgbAt(rgba, 0)).toEqual([1, 1, 1]);
    expect(rgbAt(rgba, 1)).toEqual([9, 8, 7]);
    expect(rgbAt(rgba, 2)).toEqual([3, 3, 3]);
  });

  it("dissolve divides a region between two neighbours (nearest wins)", () => {
    // [A, X, X, B] -> left X to A, right X to B.
    const A: [number, number, number] = [1, 0, 0];
    const X: [number, number, number] = [9, 9, 9];
    const B: [number, number, number] = [0, 0, 1];
    const rgba = rgbaOf([A, X, X, B]);
    applyOpsToRgba(rgba, [dissolveOp(X, [A, B])], 4, 1);
    expect(rgbAt(rgba, 0)).toEqual(A);
    expect(rgbAt(rgba, 1)).toEqual(A);
    expect(rgbAt(rgba, 2)).toEqual(B);
    expect(rgbAt(rgba, 3)).toEqual(B);
  });

  it("dissolve into a single target is a plain merge", () => {
    const A: [number, number, number] = [1, 2, 3];
    const X: [number, number, number] = [9, 9, 9];
    const rgba = rgbaOf([A, X, X]);
    applyOpsToRgba(rgba, [dissolveOp(X, [A])], 3, 1);
    expect(rgbAt(rgba, 1)).toEqual(A);
    expect(rgbAt(rgba, 2)).toEqual(A);
  });

  it("composes ops in order (carve then dissolve back)", () => {
    const A: [number, number, number] = [2, 2, 2];
    const NEW: [number, number, number] = [50, 60, 70];
    const rgba = rgbaOf([A, [8, 8, 8]]);
    applyOpsToRgba(rgba, [paintOp([1], NEW), dissolveOp(NEW, [A])], 2, 1);
    expect(rgbAt(rgba, 0)).toEqual(A);
    expect(rgbAt(rgba, 1)).toEqual(A);
  });
});

describe("op builders", () => {
  it("paintOp and dissolveOp produce the wire shapes", () => {
    expect(paintOp([1, 2], [9, 8, 7])).toEqual({
      op: "paint",
      pixels: [1, 2],
      color: [9, 8, 7],
    });
    expect(dissolveOp([5, 5, 5], [[1, 1, 1], [2, 2, 2]])).toEqual({
      op: "dissolve",
      from: [5, 5, 5],
      into: [[1, 1, 1], [2, 2, 2]],
    });
  });
});
