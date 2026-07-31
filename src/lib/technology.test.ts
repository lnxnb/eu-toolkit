import { describe, expect, it } from "vitest";
import {
  foldLevelDeletes,
  foldLevelModifiers,
  hasPendingLevelEdits,
  levelDescKey,
  levelNameKey,
  modifierStatement,
  pendingLevelDeletes,
  type TechLevel,
  type TechRow,
} from "./technology";
import type { TypedEdit } from "./edits.svelte";

const FILE = "common/technologies/mil.txt";

function row(key: string, value: string): TechRow {
  return { key, value, kind: "modifier", label: key };
}

const BASE: TechRow[] = [row("land_morale", "0.9"), row("infantry_shock", "0.1")];

describe("foldLevelModifiers", () => {
  it("passes the disk rows through when nothing is pending", () => {
    expect(foldLevelModifiers(BASE, FILE, 0, [])).toEqual(BASE);
  });

  it("overrides a value from a setScalar", () => {
    const edits: TypedEdit[] = [
      { kind: "setScalar", file: FILE, path: ["technology#0", "land_morale"], value: "1.5", quoted: false },
    ];
    const out = foldLevelModifiers(BASE, FILE, 0, edits);
    expect(out.find((r) => r.key === "land_morale")?.value).toBe("1.5");
  });

  it("adds a row from an insertStatement and marks it pending", () => {
    const edits: TypedEdit[] = [
      { kind: "insertStatement", file: FILE, blockPath: ["technology#0"], statement: modifierStatement("discipline", "0.05") },
    ];
    const out = foldLevelModifiers(BASE, FILE, 0, edits);
    expect(out).toHaveLength(3);
    expect(out[2]).toMatchObject({ key: "discipline", value: "0.05", pending: true });
  });

  it("drops a row from a removeStatement", () => {
    const edits: TypedEdit[] = [
      { kind: "removeStatement", file: FILE, blockPath: ["technology#0"], key: "land_morale" },
    ];
    const out = foldLevelModifiers(BASE, FILE, 0, edits);
    expect(out.map((r) => r.key)).toEqual(["infantry_shock"]);
  });

  it("applies a later value edit to a row added in the same session", () => {
    // Both edits compose on one evolving buffer at save time, so the read-back
    // must agree: insert first, then the setScalar wins.
    const edits: TypedEdit[] = [
      { kind: "insertStatement", file: FILE, blockPath: ["technology#0"], statement: "discipline = 0.05" },
      { kind: "setScalar", file: FILE, path: ["technology#0", "discipline"], value: "0.25", quoted: false },
    ];
    const out = foldLevelModifiers(BASE, FILE, 0, edits);
    expect(out.find((r) => r.key === "discipline")?.value).toBe("0.25");
  });

  it("ignores edits aimed at another level, file, or a unit unlock", () => {
    const edits: TypedEdit[] = [
      // Another level of the same file.
      { kind: "removeStatement", file: FILE, blockPath: ["technology#1"], key: "land_morale" },
      // Same level index in a different power's file.
      { kind: "removeStatement", file: "common/technologies/adm.txt", blockPath: ["technology#0"], key: "infantry_shock" },
      // `enable = <unit>` shares the block path but is not a modifier row.
      { kind: "insertStatement", file: FILE, blockPath: ["technology#0"], statement: "enable = hussars" },
      // A boolean unlock is not numeric.
      { kind: "insertStatement", file: FILE, blockPath: ["technology#0"], statement: "native_palisade = yes" },
    ];
    expect(foldLevelModifiers(BASE, FILE, 0, edits)).toEqual(BASE);
  });
});

function level(index: number): TechLevel {
  return {
    index,
    file: FILE,
    name: `Tech ${index}`,
    desc: null,
    year: "1400",
    modifiers: [],
    unlocks: [],
    units: [],
    rawExtra: [],
  };
}

describe("level loc keys", () => {
  it("indexes name and desc by level only", () => {
    // Verified against localisation/technology_l_english.yml: level index is the
    // only axis — names do not vary by tech group.
    expect(levelNameKey("adm", 5)).toBe("adm_tech_cs_5_name");
    expect(levelDescKey("mil", 0)).toBe("mil_tech_cs_0_desc");
  });
});

describe("level deletion", () => {
  const LEVELS = [level(0), level(1), level(2)];
  const del = (i: number): TypedEdit => ({
    kind: "removeStatement",
    file: FILE,
    blockPath: [],
    key: `technology#${i}`,
  });

  it("collects pending deletes and folds them out of the list", () => {
    expect(pendingLevelDeletes(FILE, [del(1)])).toEqual([1]);
    expect(foldLevelDeletes(LEVELS, FILE, [del(1)]).map((l) => l.index)).toEqual([0, 2]);
  });

  it("ignores a delete aimed at another file", () => {
    const other: TypedEdit = { kind: "removeStatement", file: "common/technologies/adm.txt", blockPath: [], key: "technology#1" };
    expect(pendingLevelDeletes(FILE, [other])).toEqual([]);
    expect(foldLevelDeletes(LEVELS, FILE, [other])).toEqual(LEVELS);
  });

  it("does not mistake a modifier removal for a level deletion", () => {
    // Same edit kind, but scoped INSIDE a level block rather than at the root.
    const inner: TypedEdit = { kind: "removeStatement", file: FILE, blockPath: ["technology#1"], key: "land_morale" };
    expect(pendingLevelDeletes(FILE, [inner])).toEqual([]);
  });

  it("detects the index-addressed pending edits that make a delete unsafe", () => {
    expect(hasPendingLevelEdits(FILE, [])).toBe(false);
    expect(
      hasPendingLevelEdits(FILE, [
        { kind: "setScalar", file: FILE, path: ["technology#2", "year"], value: "1500", quoted: false },
      ]),
    ).toBe(true);
    expect(
      hasPendingLevelEdits(FILE, [
        { kind: "insertStatement", file: FILE, blockPath: ["technology#0"], statement: "discipline = 0.05" },
      ]),
    ).toBe(true);
    // Appending a level shifts nothing but still writes the same file.
    expect(hasPendingLevelEdits(FILE, [{ kind: "appendText", file: FILE, text: "technology = { year = 1500 }" }])).toBe(true);
    // A loc override is index-based but touches no block, so it is safe.
    expect(hasPendingLevelEdits(FILE, [{ kind: "locOverride", key: "mil_tech_cs_1_name", value: "X" }])).toBe(false);
  });
});
