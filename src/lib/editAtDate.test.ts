import { describe, it, expect } from "vitest";
import {
  editAtDate,
  isShadowed,
  shadowedKeysFrom,
  type DatedBlockRef,
} from "./editAtDate";
import type { TypedEdit } from "./edits.svelte";

// The write-target decision. The load-bearing case is a TIMELINE mod (Extended
// Timeline & co): its history files keep a baseline epoch at the top level and
// replay history forward in dated blocks, so a top-level write at the start date
// is overridden by every intervening block and never reaches the player's world.
// The top level is authoritative only when nothing shadows the written keys.

const FILE = "history/provinces/167 - Caux.txt";

/** The `startEdits` a caller hands in — identity-compared, contents irrelevant. */
const START_EDITS: TypedEdit[] = [
  { kind: "setScalar", file: FILE, path: ["owner"], value: "ENG", quoted: false },
];

function blocks(...specs: [string, ...string[]][]): DatedBlockRef[] {
  return specs.map(([date, ...keys]) => ({ date, occurrenceIndex: 0, keys }));
}

function run(opts: {
  selectedDate: string | null;
  startDate: string;
  datedBlocks?: DatedBlockRef[];
  statements?: string[];
}): TypedEdit[] {
  const datedBlocks = opts.datedBlocks ?? [];
  return editAtDate({
    file: FILE,
    selectedDate: opts.selectedDate,
    startDate: opts.startDate,
    datedBlocks,
    startEdits: START_EDITS,
    statements: opts.statements ?? ["owner = ENG"],
    shadowedKeys: shadowedKeysFrom(datedBlocks, opts.selectedDate),
  });
}

describe("editAtDate — timeline mods (the reported defect)", () => {
  it("emits a dated block when pre-start history overrides the written key", () => {
    // Caux: top level `owner = ROM` (year-2 epoch), last pre-start owner change
    // 1204.6.24 = { owner = FRA }. Start date 1302.9.1.
    const out = run({
      selectedDate: "1302.9.1",
      startDate: "1302.9.1",
      datedBlocks: blocks(["1204.6.24", "owner", "controller"]),
    });
    expect(out).toEqual([
      {
        kind: "insertDatedBlock",
        file: FILE,
        date: "1302.9.1",
        statement: "1302.9.1 = { owner = ENG }",
      },
    ]);
  });

  it("keeps owner, controller and core together in one dated block", () => {
    const out = run({
      selectedDate: "1302.9.1",
      startDate: "1302.9.1",
      datedBlocks: blocks(["1204.6.24", "owner"]),
      statements: ["owner = ENG", "controller = ENG", "add_core = ENG"],
    });
    expect(out).toHaveLength(1);
    expect(out[0]).toMatchObject({
      kind: "insertDatedBlock",
      statement: "1302.9.1 = { owner = ENG controller = ENG add_core = ENG }",
    });
  });

  it("merges into an existing block for the start date rather than adding one", () => {
    const out = run({
      selectedDate: "1302.9.1",
      startDate: "1302.9.1",
      datedBlocks: blocks(["1204.6.24", "owner"], ["1302.9.1", "unrest"]),
    });
    expect(out).toEqual([
      { kind: "insertStatement", file: FILE, blockPath: ["1302.9.1"], statement: "owner = ENG" },
    ]);
  });

  it("addresses the LAST occurrence when the start date has repeated blocks", () => {
    const out = editAtDate({
      file: FILE,
      selectedDate: "1302.9.1",
      startDate: "1302.9.1",
      datedBlocks: [
        { date: "1204.6.24", occurrenceIndex: 0, keys: ["owner"] },
        { date: "1302.9.1", occurrenceIndex: 0, keys: ["unrest"] },
        { date: "1302.9.1", occurrenceIndex: 1, keys: ["unrest"] },
      ],
      startEdits: START_EDITS,
      statements: ["owner = ENG"],
      shadowedKeys: new Set(["owner"]),
    });
    expect(out[0]).toMatchObject({ blockPath: ["1302.9.1#1"] });
  });

  it("still writes top-level for a province whose history never touched the key", () => {
    // Same timeline mod, but this province has only an unrelated pre-start block.
    const out = run({
      selectedDate: "1302.9.1",
      startDate: "1302.9.1",
      datedBlocks: blocks(["1204.6.24", "unrest"]),
    });
    expect(out).toBe(START_EDITS);
  });

  it("ignores blocks dated after the selected date", () => {
    const out = run({
      selectedDate: "1302.9.1",
      startDate: "1302.9.1",
      datedBlocks: blocks(["1450.1.1", "owner"]),
    });
    expect(out).toBe(START_EDITS);
  });
});

describe("editAtDate — vanilla is unaffected", () => {
  it("writes top-level at the start date and emits no redundant dated block", () => {
    const out = run({ selectedDate: "1444.11.11", startDate: "1444.11.11" });
    expect(out).toBe(START_EDITS);
  });

  it("writes top-level at the start date even with post-start dated blocks", () => {
    const out = run({
      selectedDate: "1444.11.11",
      startDate: "1444.11.11",
      datedBlocks: blocks(["1453.5.29", "owner"], ["1500.1.1", "owner"]),
    });
    expect(out).toBe(START_EDITS);
  });

  it("writes top-level when the date is unresolved", () => {
    const out = run({ selectedDate: null, startDate: "1444.11.11" });
    expect(out).toBe(START_EDITS);
  });
});

describe("editAtDate — later dates keep Sprint 12.3 behaviour", () => {
  it("inserts a fresh dated block with no shadowing at all", () => {
    const out = run({ selectedDate: "1500.1.1", startDate: "1444.11.11" });
    expect(out).toEqual([
      {
        kind: "insertDatedBlock",
        file: FILE,
        date: "1500.1.1",
        statement: "1500.1.1 = { owner = ENG }",
      },
    ]);
  });

  it("merges into an existing block for that date", () => {
    const out = run({
      selectedDate: "1500.1.1",
      startDate: "1444.11.11",
      datedBlocks: blocks(["1500.1.1", "unrest"]),
    });
    expect(out).toEqual([
      { kind: "insertStatement", file: FILE, blockPath: ["1500.1.1"], statement: "owner = ENG" },
    ]);
  });

  it("is a no-op when there is nothing to write into the block", () => {
    const out = run({ selectedDate: "1500.1.1", startDate: "1444.11.11", statements: [] });
    expect(out).toEqual([]);
  });
});

describe("isShadowed — cumulative keys are not assignments", () => {
  it("does not treat a dated add_core as shadowing a top-level add_core", () => {
    // Cores stack; a baseline `add_core = CAS` survives a later `add_core = ARA`.
    expect(isShadowed(["add_core = CAS"], new Set(["add_core"]))).toBe(false);
  });

  it("treats a dated remove_core as shadowing a top-level add_core", () => {
    expect(isShadowed(["add_core = CAS"], new Set(["remove_core"]))).toBe(true);
  });

  it("never shadows discovered_by, which has no inverse", () => {
    expect(isShadowed(["discovered_by = western"], new Set(["discovered_by"]))).toBe(false);
  });

  it("shadows a plain assignment on the same key", () => {
    expect(isShadowed(["religion = catholic"], new Set(["religion"]))).toBe(true);
  });

  it("shadows the whole write when any one statement is shadowed", () => {
    expect(isShadowed(["add_core = ENG", "owner = ENG"], new Set(["owner"]))).toBe(true);
  });

  it("is false against an empty shadow set", () => {
    expect(isShadowed(["owner = ENG"], new Set())).toBe(false);
  });
});

describe("shadowedKeysFrom", () => {
  it("unions the keys of every block at or before the date", () => {
    const set = shadowedKeysFrom(
      blocks(["1204.6.24", "owner", "controller"], ["1250.1.1", "religion"]),
      "1302.9.1",
    );
    expect(set).toEqual(new Set(["owner", "controller", "religion"]));
  });

  it("includes a block dated exactly on the selected date", () => {
    expect(shadowedKeysFrom(blocks(["1302.9.1", "owner"]), "1302.9.1")).toEqual(
      new Set(["owner"]),
    );
  });

  it("excludes later blocks and is empty for an unresolved date", () => {
    expect(shadowedKeysFrom(blocks(["1450.1.1", "owner"]), "1302.9.1").size).toBe(0);
    expect(shadowedKeysFrom(blocks(["1204.6.24", "owner"]), null).size).toBe(0);
  });
});
