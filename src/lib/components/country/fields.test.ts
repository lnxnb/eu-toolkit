// Country-history date routing (the timeline-model write fix): pushAtDate must
// land a write top-level only when the top level is authoritative for the
// written key at the selected date, and the pendingHist* display folds must see
// the write whichever shape it took.
import { describe, expect, it } from "vitest";
import { EditQueue } from "$lib/edits.svelte";
import {
  blockRefs,
  pushAtDate,
  writesDatedBlock,
  pendingHistField,
  pendingHistList,
  scalarEdit,
  listAdd,
  listRemove,
  type CountryDateCtx,
} from "./fields";
import type { CountryDatedBlock } from "./history";

const FILE = "history/countries/SWE - Sweden.txt";

function block(date: string, keys: string[], occ = 0): CountryDatedBlock {
  return {
    date,
    post_start: false,
    occurrence_index: occ,
    entries: keys.map((key) => ({ key, value: "x", is_block: false })),
  };
}

function ctx(
  selectedDate: string | null,
  startDate: string,
  blocks: CountryDatedBlock[] = [],
): CountryDateCtx {
  return { file: FILE, selectedDate, startDate, blocks };
}

describe("pushAtDate — vanilla model unchanged", () => {
  it("writes top-level at the start date when nothing shadows the key", () => {
    const q = new EditQueue();
    pushAtDate(
      q,
      ctx("1444.11.11", "1444.11.11", [block("1450.1.1", ["government"])]),
      "set gov",
      [scalarEdit(FILE, "government", "republic", true)],
      ["government = republic"],
    );
    const edits = q.serialize();
    expect(edits).toEqual([
      { kind: "setScalar", file: FILE, path: ["government"], value: "republic", quoted: false },
    ]);
    // Top-level baseline write: the composite is not date-tagged.
    expect(q.composites[0].date).toBeUndefined();
  });
});

describe("pushAtDate — timeline model (the reported defect)", () => {
  // Extended-Timeline shape: start 1302.9.1, top level is the year-2 baseline,
  // a 1204 block already assigns the key. A top-level write would be silently
  // overridden — the write must become a dated block at the selected date.
  it("writes a dated block at the start date when the key is shadowed", () => {
    const q = new EditQueue();
    pushAtDate(
      q,
      ctx("1302.9.1", "1302.9.1", [block("1204.6.24", ["government", "religion"])]),
      "set gov",
      [scalarEdit(FILE, "government", "republic", true)],
      ["government = republic"],
    );
    const edits = q.serialize();
    expect(edits).toEqual([
      {
        kind: "insertDatedBlock",
        file: FILE,
        date: "1302.9.1",
        statement: "1302.9.1 = { government = republic }",
      },
    ]);
    expect(q.composites[0].date).toBe("1302.9.1");
  });

  it("merges into an existing exact-date block", () => {
    const q = new EditQueue();
    pushAtDate(
      q,
      ctx("1302.9.1", "1302.9.1", [
        block("1204.6.24", ["government"]),
        block("1302.9.1", ["mercantilism"]),
      ]),
      "set gov",
      [scalarEdit(FILE, "government", "republic", true)],
      ["government = republic"],
    );
    expect(q.serialize()).toEqual([
      {
        kind: "insertStatement",
        file: FILE,
        blockPath: ["1302.9.1"],
        statement: "government = republic",
      },
    ]);
  });

  it("keeps unshadowed keys top-level even when other keys are shadowed", () => {
    const q = new EditQueue();
    pushAtDate(
      q,
      ctx("1302.9.1", "1302.9.1", [block("1204.6.24", ["government"])]),
      "set merc",
      [scalarEdit(FILE, "mercantilism", "10", true)],
      ["mercantilism = 10"],
    );
    expect(q.serialize()[0].kind).toBe("setScalar");
  });

  it("routes a later-date write into a dated block regardless of shadowing", () => {
    const q = new EditQueue();
    pushAtDate(
      q,
      ctx("1500.1.1", "1444.11.11"),
      "set gov",
      [scalarEdit(FILE, "government", "republic", true)],
      ["government = republic"],
    );
    expect(q.serialize()[0].kind).toBe("insertDatedBlock");
  });
});

describe("writesDatedBlock — cumulative inverses", () => {
  it("a grant is shadowed only by its inverse, not by other grants", () => {
    const blocks = [block("1204.6.24", ["set_estate_privilege"])];
    // Another grant does not cancel a top-level grant (cumulative)…
    expect(
      writesDatedBlock(ctx("1302.9.1", "1302.9.1", blocks), [
        "set_estate_privilege = estate_nobles_wartaxes",
      ]),
    ).toBe(false);
    // …but a pre-start revoke does.
    const revoked = [block("1204.6.24", ["remove_estate_privilege"])];
    expect(
      writesDatedBlock(ctx("1302.9.1", "1302.9.1", revoked), [
        "set_estate_privilege = estate_nobles_wartaxes",
      ]),
    ).toBe(true);
  });

  it("an accepted-culture removal is shadowed by a pre-start dated add", () => {
    const blocks = [block("1204.6.24", ["add_accepted_culture"])];
    expect(
      writesDatedBlock(ctx("1302.9.1", "1302.9.1", blocks), [
        "remove_accepted_culture = finnish",
      ]),
    ).toBe(true);
  });
});

describe("pendingHistField — display fold", () => {
  it("sees top-level, dated-merge, and fresh dated-block writes (last wins)", () => {
    const q = new EditQueue();
    q.push({
      label: "a",
      edits: [{ kind: "setScalar", file: FILE, path: ["government"], value: "republic", quoted: false }],
    });
    expect(pendingHistField(q, FILE, "government", "1444.11.11")?.value).toBe("republic");
    q.push({
      label: "b",
      edits: [
        {
          kind: "insertDatedBlock",
          file: FILE,
          date: "1444.11.11",
          statement: "1444.11.11 = { government = theocracy }",
        },
      ],
      date: "1444.11.11",
    });
    expect(pendingHistField(q, FILE, "government", "1444.11.11")?.value).toBe("theocracy");
    q.push({
      label: "c",
      edits: [
        {
          kind: "insertStatement",
          file: FILE,
          blockPath: ["1444.11.11"],
          statement: "government = monarchy",
        },
      ],
      date: "1444.11.11",
    });
    expect(pendingHistField(q, FILE, "government", "1444.11.11")?.value).toBe("monarchy");
  });

  it("ignores dated writes after the selected date and nested holder scalars", () => {
    const q = new EditQueue();
    q.push({
      label: "later",
      edits: [
        {
          kind: "insertDatedBlock",
          file: FILE,
          date: "1500.1.1",
          statement: "1500.1.1 = { government = republic }",
        },
      ],
      date: "1500.1.1",
    });
    expect(pendingHistField(q, FILE, "government", "1444.11.11")).toBeUndefined();
    // A holder create's religion override must not read as country religion.
    q.push({
      label: "ruler",
      edits: [
        {
          kind: "insertDatedBlock",
          file: FILE,
          date: "1444.11.11",
          statement:
            '1444.11.11 = { monarch = { name = "Karl" religion = catholic leader = { fire = 3 } } }',
        },
      ],
      date: "1444.11.11",
    });
    expect(pendingHistField(q, FILE, "religion", "1444.11.11")).toBeUndefined();
  });

  it("reports a pending top-level removal as null", () => {
    const q = new EditQueue();
    q.push({
      label: "clear",
      edits: [{ kind: "removeStatement", file: FILE, blockPath: [], key: "national_focus" }],
    });
    expect(pendingHistField(q, FILE, "national_focus", "1444.11.11")).toEqual({ value: null });
  });
});

describe("pendingHistList — display fold", () => {
  it("folds top-level and dated adds plus inverse removals", () => {
    const q = new EditQueue();
    q.push({ label: "add", edits: [listAdd(FILE, "add_accepted_culture", "finnish")] });
    q.push({
      label: "dated add",
      edits: [
        {
          kind: "insertDatedBlock",
          file: FILE,
          date: "1444.11.11",
          statement: "1444.11.11 = { add_accepted_culture = norwegian }",
        },
      ],
      date: "1444.11.11",
    });
    expect(
      pendingHistList(q, FILE, "add_accepted_culture", ["gotlandic"], "1444.11.11", "remove_accepted_culture"),
    ).toEqual(["gotlandic", "finnish", "norwegian"]);
    // Dated inverse subtracts a base entry.
    q.push({
      label: "dated remove",
      edits: [
        {
          kind: "insertDatedBlock",
          file: FILE,
          date: "1444.11.11",
          statement: "1444.11.11 = { remove_accepted_culture = gotlandic }",
        },
      ],
      date: "1444.11.11",
    });
    expect(
      pendingHistList(q, FILE, "add_accepted_culture", ["gotlandic"], "1444.11.11", "remove_accepted_culture"),
    ).toEqual(["finnish", "norwegian"]);
    // Top-level value-filtered removal still folds.
    q.push({ label: "remove", edits: [listRemove(FILE, "add_accepted_culture", "finnish")] });
    expect(
      pendingHistList(q, FILE, "add_accepted_culture", ["gotlandic"], "1444.11.11", "remove_accepted_culture"),
    ).toEqual(["norwegian"]);
  });
});

describe("blockRefs", () => {
  it("maps the backend dated-block shape onto DatedBlockRef", () => {
    expect(blockRefs([block("1204.6.24", ["owner", "government"], 1)])).toEqual([
      { date: "1204.6.24", occurrenceIndex: 1, keys: ["owner", "government"] },
    ]);
  });
});
