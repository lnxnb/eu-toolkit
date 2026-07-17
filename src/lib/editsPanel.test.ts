import { describe, it, expect } from "vitest";
import {
  editFiles,
  compositeFiles,
  isIndependentlyRevertible,
  compositeJump,
  summarizeEdit,
} from "./editsPanel";
import type { Composite, TypedEdit } from "$lib/edits.svelte";

function comp(label: string, edits: TypedEdit[]): Composite {
  return { label, edits };
}

// Convenience edit builders.
const setScalar = (file: string, path: string[], value = "x"): TypedEdit => ({
  kind: "setScalar",
  file,
  path,
  value,
  quoted: false,
});
const addId = (file: string, listPath: string[], id: string): TypedEdit => ({
  kind: "addId",
  file,
  listPath,
  id,
});

describe("editFiles / compositeFiles", () => {
  it("maps a listMove to both endpoints", () => {
    const e: TypedEdit = {
      kind: "listMove",
      fromFile: "map/area.txt",
      fromPath: ["a", "x"],
      toFile: "map/region.txt",
      toPath: ["b", "x"],
      id: "5",
    };
    expect(editFiles(e).sort()).toEqual(["map/area.txt", "map/region.txt"]);
  });

  it("collapses a same-file listMove to one key", () => {
    const e: TypedEdit = {
      kind: "listMove",
      fromFile: "map/area.txt",
      fromPath: ["a"],
      toFile: "map/area.txt",
      toPath: ["b"],
      id: "5",
    };
    expect(editFiles(e)).toEqual(["map/area.txt"]);
  });

  it("keys defines and localisation on shared synthetic files", () => {
    expect(editFiles({ kind: "setDefine", key: "K", value: "1" })).toEqual(["<defines>"]);
    expect(editFiles({ kind: "locOverride", key: "K", value: "v" })).toEqual(["<localisation>"]);
    expect(editFiles({ kind: "locRemove", key: "K" })).toEqual(["<localisation>"]);
  });

  it("keys a ruler rename per-tag", () => {
    expect(editFiles({ kind: "renameRuler", tag: "FRA", name: "Bob" })).toEqual(["<ruler:FRA>"]);
  });

  it("unions every edit's files in a composite", () => {
    const c = comp("x", [setScalar("a.txt", ["k"]), addId("b.txt", ["l"], "1")]);
    expect([...compositeFiles(c)].sort()).toEqual(["a.txt", "b.txt"]);
  });
});

describe("isIndependentlyRevertible", () => {
  it("is true when no later composite touches the same file", () => {
    const q = [
      comp("A", [setScalar("a.txt", ["k"])]),
      comp("B", [setScalar("b.txt", ["k"])]), // target: different file
      comp("C", [setScalar("c.txt", ["k"])]),
    ];
    expect(isIndependentlyRevertible(q, 1)).toBe(true);
  });

  it("is false when a LATER composite touches the same file", () => {
    const q = [
      comp("A", [setScalar("shared.txt", ["k"])]), // target
      comp("B", [setScalar("shared.txt", ["k2"])]), // later, same file
    ];
    expect(isIndependentlyRevertible(q, 0)).toBe(false);
  });

  it("is true when only an EARLIER composite shares the file", () => {
    // Earlier same-file is allowed: dropping the later one leaves a valid queue.
    const q = [
      comp("A", [setScalar("shared.txt", ["k"])]), // earlier, same file
      comp("B", [setScalar("shared.txt", ["k2"])]), // target (last)
    ];
    expect(isIndependentlyRevertible(q, 1)).toBe(true);
  });

  it("the last composite is always revertible alone", () => {
    const q = [
      comp("A", [setScalar("a.txt", ["k"])]),
      comp("B", [setScalar("a.txt", ["k"])]),
      comp("C", [setScalar("a.txt", ["k"])]),
    ];
    expect(isIndependentlyRevertible(q, 2)).toBe(true);
  });

  it("detects a later listMove endpoint collision", () => {
    const q = [
      comp("A", [addId("map/area.txt", ["a1", "members"], "5")]), // target
      comp("B", [
        {
          kind: "listMove",
          fromFile: "map/region.txt",
          fromPath: ["r1", "areas"],
          toFile: "map/area.txt", // collides with A via toFile
          toPath: ["a2", "members"],
          id: "6",
        },
      ]),
    ];
    expect(isIndependentlyRevertible(q, 0)).toBe(false);
  });

  it("treats two localisation composites as dependent (shared writer)", () => {
    const q = [
      comp("A", [{ kind: "locOverride", key: "K1", value: "a" }]), // target
      comp("B", [{ kind: "locOverride", key: "K2", value: "b" }]), // later, same writer
    ];
    expect(isIndependentlyRevertible(q, 0)).toBe(false);
  });

  it("treats different-tag ruler renames as independent", () => {
    const q = [
      comp("A", [{ kind: "renameRuler", tag: "FRA", name: "Bob" }]),
      comp("B", [{ kind: "renameRuler", tag: "ENG", name: "Al" }]),
    ];
    expect(isIndependentlyRevertible(q, 0)).toBe(true);
  });

  it("returns false for out-of-range indices", () => {
    const q = [comp("A", [setScalar("a.txt", ["k"])])];
    expect(isIndependentlyRevertible(q, -1)).toBe(false);
    expect(isIndependentlyRevertible(q, 5)).toBe(false);
  });
});

describe("compositeJump", () => {
  it("routes a province-history edit to the province", () => {
    const c = comp("Paint", [setScalar("history/provinces/151 - Paris.txt", ["owner"], "FRA")]);
    expect(compositeJump(c)).toEqual({ kind: "province", id: 151 });
  });

  it("routes a ruler rename to the country", () => {
    const c = comp("Rename", [{ kind: "renameRuler", tag: "CAS", name: "X" }]);
    expect(compositeJump(c)).toEqual({ kind: "country", tag: "CAS" });
  });

  it("routes a country-history edit to the tag", () => {
    const c = comp("Edit", [setScalar("history/countries/FRA - France.txt", ["government"], "monarchy")]);
    expect(compositeJump(c)).toEqual({ kind: "country", tag: "FRA" });
  });

  it("routes a static area file to the areas mode", () => {
    const c = comp("Area", [addId("map/area.txt", ["france_area", "members"], "151")]);
    expect(compositeJump(c)).toEqual({ kind: "mode", mode: "areas" });
  });

  it("routes a tradenodes file to trade_nodes mode", () => {
    const c = comp("TN", [addId("common/tradenodes/00_tradenodes.txt", ["genua", "members"], "112")]);
    expect(compositeJump(c)).toEqual({ kind: "mode", mode: "trade_nodes" });
  });

  it("returns null for defines-only composites", () => {
    const c = comp("Def", [{ kind: "setDefine", key: "START_DATE", value: "1400.1.1" }]);
    expect(compositeJump(c)).toBeNull();
  });
});

describe("summarizeEdit", () => {
  it("renders a setScalar with its path and value", () => {
    const s = summarizeEdit(setScalar("history/provinces/1.txt", ["owner"], "FRA"));
    expect(s.file).toBe("history/provinces/1.txt");
    expect(s.detail).toBe("set owner = FRA");
  });

  it("renders a listMove with both files", () => {
    const s = summarizeEdit({
      kind: "listMove",
      fromFile: "map/area.txt",
      fromPath: ["a", "members"],
      toFile: "map/region.txt",
      toPath: ["b", "areas"],
      id: "5",
    });
    expect(s.file).toBe("map/area.txt → map/region.txt");
    expect(s.detail).toContain("move 5");
  });
});
