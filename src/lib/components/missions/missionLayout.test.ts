import { describe, it, expect } from "vitest";
import {
  composeBoard,
  combinedEdges,
  combinedCreatesCycle,
  clampSlot,
} from "./missionLayout";
import type { MissionSeries, MissionEntry } from "./missionsTypes";

// --- Fixture builders --------------------------------------------------------

function mission(key: string, position: number | null, requires: string[] = []): MissionEntry {
  return {
    key,
    icon: null,
    position,
    ordinal: 0,
    effectivePosition: position ?? 0,
    requiredMissions: requires,
    completedBy: null,
    title: key,
    titleKey: `${key}_title`,
    descKey: `${key}_desc`,
    titleLoc: null,
    descLoc: null,
    path: ["s", key],
    triggerPath: ["s", key, "trigger"],
    effectPath: ["s", key, "effect"],
    provincesPath: ["s", key, "provinces_to_highlight"],
    requiredPath: ["s", key, "required_missions"],
    hasTrigger: false,
    hasEffect: false,
    hasProvinces: false,
    hasRequiredBlock: true,
  };
}

function series(
  key: string,
  slot: number | null,
  missions: MissionEntry[],
  extra: Partial<MissionSeries> = {},
): MissionSeries {
  return {
    key,
    file: `missions/${key}.txt`,
    origin: "base",
    slot,
    generic: false,
    ai: true,
    hasCountryShield: true,
    hasPotential: true,
    path: [key],
    potentialPath: [key, "potential"],
    missions,
    ...extra,
  };
}

// The Aachen slot-1 reality: three series stack in one column with disjoint,
// sequential position ranges (group_1 positionless → 1..4, group_2 pos=5,
// group_3 pos=6 then continuing 7,8).
function westphalian(): MissionSeries[] {
  return [
    series("westfalian_group_1", 1, [
      mission("wes_enforce_sovereignty", null),
      mission("wes_conquest_of_westfalen", null, ["wes_enforce_sovereignty"]),
      mission("wes_unite_westphalia", null, ["wes_conquest_of_westfalen"]),
      mission("wes_widukind", null),
    ]),
    series("westfalian_group_2", 1, [mission("wes_hre_peace", 5)]),
    series("westfalian_group_3", 1, [
      mission("wes_rheinland_start", 6),
      mission("wes_rheinland_mid", null, ["wes_rheinland_start"]),
      mission("wes_rheinland_end", null, ["wes_rheinland_mid"]),
    ]),
  ];
}

// --- clampSlot ---------------------------------------------------------------

describe("clampSlot", () => {
  it("defaults absent/invalid to 1 and clamps to 1..5", () => {
    expect(clampSlot(null)).toBe(1);
    expect(clampSlot(undefined)).toBe(1);
    expect(clampSlot(0)).toBe(1);
    expect(clampSlot(3)).toBe(3);
    expect(clampSlot(9)).toBe(5);
    expect(clampSlot(NaN)).toBe(1);
  });
});

// --- Slot grouping + sequential stacking + global rows -----------------------

describe("composeBoard slot stacking", () => {
  it("stacks same-slot series into disjoint sequential global rows", () => {
    const { nodes, maxRow } = composeBoard(westphalian());
    const rowOf = (k: string) => nodes.find((n) => n.key === k)!.row;
    // group_1 positionless → running rows 1..4
    expect(rowOf("wes_enforce_sovereignty")).toBe(1);
    expect(rowOf("wes_conquest_of_westfalen")).toBe(2);
    expect(rowOf("wes_unite_westphalia")).toBe(3);
    expect(rowOf("wes_widukind")).toBe(4);
    // group_2 explicit position 5
    expect(rowOf("wes_hre_peace")).toBe(5);
    // group_3 explicit 6, then positionless continue 7, 8
    expect(rowOf("wes_rheinland_start")).toBe(6);
    expect(rowOf("wes_rheinland_mid")).toBe(7);
    expect(rowOf("wes_rheinland_end")).toBe(8);
    // all in column 0 (slot 1)
    expect(nodes.every((n) => n.col === 0)).toBe(true);
    expect(maxRow).toBe(8);
  });

  it("places distinct slots in distinct columns and aligns rows across columns", () => {
    const s = [
      series("a", 1, [mission("a1", 1), mission("a2", 3)]),
      series("b", 2, [mission("b1", 3)]),
    ];
    const { nodes } = composeBoard(s);
    const n = (k: string) => nodes.find((x) => x.key === k)!;
    expect(n("a1").col).toBe(0);
    expect(n("b1").col).toBe(1);
    // Position 3 aligns to the same row across columns.
    expect(n("a2").row).toBe(3);
    expect(n("b1").row).toBe(3);
  });

  it("bumps residual collisions when explicit positions clash", () => {
    const s = [
      series("a", 1, [mission("a1", 2), mission("a2", 2)]),
    ];
    const { nodes } = composeBoard(s);
    const rows = nodes.map((n) => n.row).sort();
    expect(rows).toEqual([2, 3]);
  });
});

// --- Series sections + add-cell attribution ----------------------------------

describe("composeBoard sections + add-cells", () => {
  it("builds one section per series with correct bands and first flag", () => {
    const { sections } = composeBoard(westphalian());
    expect(sections.map((s) => s.seriesKey)).toEqual([
      "westfalian_group_1",
      "westfalian_group_2",
      "westfalian_group_3",
    ]);
    expect(sections[0]).toMatchObject({ minRow: 1, maxRow: 4, first: true });
    expect(sections[1]).toMatchObject({ minRow: 5, maxRow: 5, first: false });
    expect(sections[2]).toMatchObject({ minRow: 6, maxRow: 8, first: false });
  });

  it("attributes the trailing add-cell to the last series in the slot", () => {
    const { addCells } = composeBoard(westphalian());
    const trailing = addCells.find((c) => c.row === 9);
    expect(trailing).toBeDefined();
    expect(trailing!.seriesIndex).toBe(2); // westfalian_group_3
  });

  it("reserves a row + add-cell for an empty series", () => {
    const s = [
      series("a", 1, [mission("a1", 1)]),
      series("empty", 1, []),
    ];
    const { sections, addCells } = composeBoard(s);
    const emptySec = sections.find((x) => x.seriesKey === "empty")!;
    expect(emptySec.minRow).toBe(2);
    expect(addCells.some((c) => c.seriesIndex === 1 && c.row === 2)).toBe(true);
  });
});

// --- Cross-series arrow resolution -------------------------------------------

describe("composeBoard arrows", () => {
  it("resolves same-column requirement arrows", () => {
    const { arrows } = composeBoard(westphalian());
    const a = arrows.find((x) => x.toKey === "wes_rheinland_mid")!;
    expect(a.fromKey).toBe("wes_rheinland_start");
    expect(a.fromRow).toBe(6);
    expect(a.toRow).toBe(7);
    expect(a.cross).toBe(false);
  });

  it("resolves cross-column arrows and flags them", () => {
    const s = [
      series("base", 1, [mission("root_m", 1)]),
      series("imperial", 2, [mission("imp_m", 1, ["root_m"])]),
    ];
    const { arrows } = composeBoard(s);
    const a = arrows.find((x) => x.toKey === "imp_m")!;
    expect(a.fromKey).toBe("root_m");
    expect(a.fromCol).toBe(0);
    expect(a.toCol).toBe(1);
    expect(a.cross).toBe(true);
  });

  it("emits an external stub for a requirement in no displayed series", () => {
    const s = [series("a", 1, [mission("a1", 1, ["missing_mission"])])];
    const { arrows, externals } = composeBoard(s);
    expect(arrows).toHaveLength(0);
    expect(externals).toEqual([
      { seriesIndex: 0, nodeKey: "a1", missingKey: "missing_mission" },
    ]);
  });
});

// --- Combined cycle detection ------------------------------------------------

describe("combinedCreatesCycle", () => {
  it("detects cycles across the union of all displayed series", () => {
    const s = [
      series("s1", 1, [mission("m1", 1), mission("m2", 2, ["m1"])]),
      series("s2", 2, [mission("m3", 1, ["m2"])]),
    ];
    const edges = combinedEdges(s);
    // m1 → m2 → m3 chain (across series). Linking m3 as prereq of m1 closes it.
    expect(combinedCreatesCycle(edges, "m1", "m3")).toBe(true);
    // self-link
    expect(combinedCreatesCycle(edges, "m2", "m2")).toBe(true);
    // a fresh forward cross-series link is fine
    expect(combinedCreatesCycle(edges, "m3", "m1")).toBe(false);
    // a brand-new mission requiring an existing one is acyclic
    expect(combinedCreatesCycle(edges, "brand_new", "m1")).toBe(false);
  });

  it("merges duplicate keys' requirements when building edges", () => {
    const s = [
      series("s1", 1, [mission("dup", 1, ["x"])]),
      series("s2", 2, [mission("dup", 1, ["y"])]),
    ];
    const edges = combinedEdges(s);
    expect(edges.get("dup")).toEqual(["x", "y"]);
  });
});
