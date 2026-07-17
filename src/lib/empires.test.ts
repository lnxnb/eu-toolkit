import { describe, it, expect } from "vitest";
import { editAtDate } from "./editAtDate";
import {
  foldMembers,
  foldElectors,
  foldEmperorTimeline,
  emperorAddEdits,
  emperorEditEdits,
  emperorRemoveEdits,
  type HreMembers,
  type Elector,
  type EmperorTimeline,
} from "./empires";
import type { TypedEdit } from "$lib/edits.svelte";

// --- Stroke → dated-block edit shape (the HRE brush) -------------------------
// The brush writes a positive `hre = yes` / `hre = no` statement per province;
// `strokeEditsAtDate` routes it through `editAtDate`. At the start date it stays
// top-level; at a later date it becomes a date-ordered `Y.M.D = { … }` block.
describe("HRE brush stroke → dated-block edit shape", () => {
  const startEdits: TypedEdit[] = [
    { kind: "insertStatement", file: "history/provinces/1 - A.txt", blockPath: [], statement: "hre = yes" },
  ];

  it("writes top-level at the start date", () => {
    const out = editAtDate({
      file: "history/provinces/1 - A.txt",
      selectedDate: "1444.11.11",
      startDate: "1444.11.11",
      datedBlocks: [],
      startEdits,
      statements: ["hre = yes"],
    });
    expect(out).toEqual(startEdits);
  });

  it("writes a date-ordered dated block at a later date", () => {
    const out = editAtDate({
      file: "history/provinces/1 - A.txt",
      selectedDate: "1500.1.1",
      startDate: "1444.11.11",
      datedBlocks: [],
      startEdits,
      statements: ["hre = yes"],
    });
    expect(out).toEqual([
      { kind: "insertDatedBlock", file: "history/provinces/1 - A.txt", date: "1500.1.1", statement: "1500.1.1 = { hre = yes }" },
    ]);
  });

  it("removal at a later date is a positive hre = no in the dated block", () => {
    const out = editAtDate({
      file: "history/provinces/2 - B.txt",
      selectedDate: "1600.6.1",
      startDate: "1444.11.11",
      datedBlocks: [],
      startEdits: [],
      statements: ["hre = no"],
    });
    expect(out).toEqual([
      { kind: "insertDatedBlock", file: "history/provinces/2 - B.txt", date: "1600.6.1", statement: "1600.6.1 = { hre = no }" },
    ]);
  });
});

// --- Members fold: the brush edits update the Members-tab count/id set --------
describe("foldMembers", () => {
  const base: HreMembers = { provinceCount: 1, provinceIds: [2], date: "1444.11.11" };

  it("adds a province from a dated hre = yes and removes one via hre = no", () => {
    const edits: TypedEdit[] = [
      { kind: "insertDatedBlock", file: "history/provinces/1 - A.txt", date: "1500.1.1", statement: "1500.1.1 = { hre = yes }" },
      { kind: "insertDatedBlock", file: "history/provinces/2 - B.txt", date: "1500.1.1", statement: "1500.1.1 = { hre = no }" },
    ];
    const out = foldMembers(base, edits);
    expect(out.provinceIds).toEqual([1]);
    expect(out.provinceCount).toBe(1);
  });

  it("honors a top-level insert and a setScalar", () => {
    const edits: TypedEdit[] = [
      { kind: "insertStatement", file: "history/provinces/3 - C.txt", blockPath: [], statement: "hre = yes" },
      { kind: "setScalar", file: "history/provinces/2 - B.txt", path: ["hre"], value: "no", quoted: false },
    ];
    const out = foldMembers(base, edits);
    expect(out.provinceIds).toEqual([3]);
  });
});

// --- Electors fold: mirrors the country-panel toggle's edit shapes -----------
describe("foldElectors", () => {
  const base: Elector[] = [{ tag: "BOH", name: "Bohemia" }];
  const nameOf = (t: string) => ({ SAX: "Saxony", BOH: "Bohemia" })[t] ?? t;

  it("adds via insertStatement and removes via removeStatement", () => {
    const edits: TypedEdit[] = [
      { kind: "insertStatement", file: "history/countries/SAX - Saxony.txt", blockPath: [], statement: "elector = yes" },
      { kind: "removeStatement", file: "history/countries/BOH - Bohemia.txt", blockPath: [], key: "elector" },
    ];
    const out = foldElectors(base, edits, nameOf);
    expect(out.map((e) => e.tag)).toEqual(["SAX"]);
  });
});

// --- Emperor timeline fold: add / edit / remove ------------------------------
describe("foldEmperorTimeline", () => {
  const base: EmperorTimeline = {
    kind: "hre",
    emperorKey: "emperor",
    writeFile: "history/diplomacy/hre.txt",
    writeFileExists: true,
    entries: [
      { date: "1437.12.9", tag: "HAB", name: "Austria", file: "history/diplomacy/hre.txt", occurrenceIndex: 0, postSelected: false, validTag: true, isSubject: false },
    ],
    date: "1444.11.11",
    current: "HAB",
    currentName: "Austria",
  };

  it("add + edit + remove reflect in the folded entries", () => {
    const add = emperorAddEdits(base, "1500.1.1", "SPA");
    expect(add).toEqual([{ kind: "insertDatedBlock", file: "history/diplomacy/hre.txt", date: "1500.1.1", statement: "1500.1.1 = { emperor = SPA }" }]);
    const added = foldEmperorTimeline(base, add, "1600.1.1");
    expect(added.entries.map((e) => e.tag)).toEqual(["HAB", "SPA"]);
    expect(added.current).toBe("SPA");

    const edit = emperorEditEdits(base, base.entries[0], "TUS");
    const edited = foldEmperorTimeline(base, edit, "1444.11.11");
    expect(edited.entries[0].tag).toBe("TUS");

    const rem = emperorRemoveEdits(base, base.entries[0]);
    const removed = foldEmperorTimeline(base, rem, "1444.11.11");
    expect(removed.entries.length).toBe(0);
    expect(removed.current).toBeNull();
  });

  it("createFile add targets the canonical file when it doesn't exist", () => {
    const fresh: EmperorTimeline = { ...base, writeFileExists: false, entries: [] };
    const add = emperorAddEdits(fresh, "1000.1.1", "ANB");
    expect(add).toEqual([{ kind: "createFile", file: "history/diplomacy/hre.txt", text: "1000.1.1 = { emperor = ANB }\n" }]);
    const folded = foldEmperorTimeline(fresh, add, "1444.11.11");
    expect(folded.current).toBe("ANB");
  });
});
