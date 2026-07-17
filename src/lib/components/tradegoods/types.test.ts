import { describe, expect, it } from "vitest";
import { goodKeyOfGroup, UNKNOWN_KEY } from "./types";

// Undiscovered spawn clusters (backend goods_spawn) key their mode-data groups
// `unknown#<n>`; everything list/paint-shaped reduces a group key to its base
// good key. Real good keys are Clausewitz identifiers and never contain `#`.
describe("goodKeyOfGroup", () => {
  it("passes plain good keys through", () => {
    expect(goodKeyOfGroup("grain")).toBe("grain");
    expect(goodKeyOfGroup(UNKNOWN_KEY)).toBe(UNKNOWN_KEY);
  });

  it("reduces cluster keys to the base good", () => {
    expect(goodKeyOfGroup("unknown#0")).toBe(UNKNOWN_KEY);
    expect(goodKeyOfGroup("unknown#412")).toBe(UNKNOWN_KEY);
  });
});
