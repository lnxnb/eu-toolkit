import { describe, it, expect } from "vitest";
import { shouldApplySelection, type Selection } from "./selectionGuard";

describe("shouldApplySelection", () => {
  it("applies a response that matches the latest selection", () => {
    const cur: Selection = { seq: 3, tag: "ENG" };
    expect(shouldApplySelection({ seq: 3, tag: "ENG" }, cur)).toBe(true);
  });

  it("drops a superseded response (newer selection since)", () => {
    // Slow ENG response arrives after the user clicked ARB (seq bumped).
    const cur: Selection = { seq: 4, tag: "ARB" };
    expect(shouldApplySelection({ seq: 3, tag: "ENG" }, cur)).toBe(false);
  });

  it("drops a response whose tag no longer matches even at the same seq", () => {
    // Defensive: tag mismatch is never applied.
    const cur: Selection = { seq: 5, tag: "ARB" };
    expect(shouldApplySelection({ seq: 5, tag: "ENG" }, cur)).toBe(false);
  });

  it("never applies when there is no current selection", () => {
    const cur: Selection = { seq: 0, tag: null };
    expect(shouldApplySelection({ seq: 0, tag: null }, cur)).toBe(false);
  });

  it("re-selecting the same tag with a fresh seq applies", () => {
    const cur: Selection = { seq: 7, tag: "ENG" };
    expect(shouldApplySelection({ seq: 7, tag: "ENG" }, cur)).toBe(true);
    // The earlier request for the same tag (older seq) is stale.
    expect(shouldApplySelection({ seq: 6, tag: "ENG" }, cur)).toBe(false);
  });
});
