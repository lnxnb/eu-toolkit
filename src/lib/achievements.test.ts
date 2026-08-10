import { describe, expect, it } from "vitest";
import {
  ACHIEVEMENTS_FILE,
  allKeys,
  foldAchievements,
  isValidKey,
  parseScaffoldAchievement,
  slugify,
  type AchievementsData,
} from "./achievements";
import type { TypedEdit } from "./edits.svelte";

const SCAFFOLD = `my_ach = {
\tid = 374
\tlocalization = my_ach
\t
\tpossible = {
\t\tironman = yes
\t}
\t
\thappened = {
\t\talways = no
\t}
}`;

function base(): AchievementsData {
  return {
    file: ACHIEVEMENTS_FILE,
    achievements: [
      {
        key: "achievement_existing",
        file: ACHIEVEMENTS_FILE,
        origin: "base",
        id: 1,
        localization: "NEW_ACHIEVEMENT_1_2",
        name: "For the Glory",
        nameKey: "NEW_ACHIEVEMENT_1_2_NAME",
        nameLoc: "For the Glory",
        descKey: "NEW_ACHIEVEMENT_1_2_DESC",
        descLoc: "Diplo-annex a vassal.",
        scriptBlocks: [
          { name: "possible", present: true },
          { name: "happened", present: true },
          { name: "visible", present: false },
          { name: "provinces_to_highlight", present: false },
        ],
        hasIcon: true,
        rawExtra: [],
        raw: "{ … }",
      },
    ],
  };
}

describe("key helpers", () => {
  it("validates and slugifies", () => {
    expect(isValidKey("my_ach_2")).toBe(true);
    expect(isValidKey("2bad")).toBe(false);
    expect(isValidKey("Bad")).toBe(false);
    expect(slugify("Sun God's Chosen!")).toBe("sun_god_s_chosen");
    expect(slugify("")).toBe("key");
  });
});

describe("parseScaffoldAchievement", () => {
  it("extracts key, id, localization and block presence", () => {
    const a = parseScaffoldAchievement(SCAFFOLD);
    expect(a).not.toBeNull();
    expect(a!.key).toBe("my_ach");
    expect(a!.id).toBe(374);
    expect(a!.localization).toBe("my_ach");
    expect(a!.nameKey).toBe("my_ach_NAME");
    expect(a!.descKey).toBe("my_ach_DESC");
    const present = a!.scriptBlocks.filter((s) => s.present).map((s) => s.name);
    expect(present).toEqual(["possible", "happened"]);
    expect(a!.hasIcon).toBe(false);
    expect(a!.origin).toBe("mod");
  });
});

describe("foldAchievements", () => {
  it("applies pending create and delete", () => {
    const edits: TypedEdit[] = [
      { kind: "appendText", file: ACHIEVEMENTS_FILE, text: "\n" + SCAFFOLD + "\n" },
    ];
    let data = foldAchievements(base(), edits);
    expect(data.achievements.map((a) => a.key)).toEqual(["achievement_existing", "my_ach"]);
    expect(allKeys(data).has("my_ach")).toBe(true);

    edits.push({ kind: "removeStatement", file: ACHIEVEMENTS_FILE, blockPath: [], key: "my_ach" });
    data = foldAchievements(base(), edits);
    expect(data.achievements.map((a) => a.key)).toEqual(["achievement_existing"]);
  });

  it("ignores edits to other files and nested removes", () => {
    const edits: TypedEdit[] = [
      { kind: "appendText", file: "common/rebel_types/x.txt", text: SCAFFOLD },
      {
        kind: "removeStatement",
        file: ACHIEVEMENTS_FILE,
        blockPath: ["achievement_existing"],
        key: "id",
      },
    ];
    const data = foldAchievements(base(), edits);
    expect(data.achievements).toHaveLength(1);
  });

  it("does not duplicate an already-present key", () => {
    const dup = SCAFFOLD.replace("my_ach = {", "achievement_existing = {");
    const data = foldAchievements(base(), [
      { kind: "appendText", file: ACHIEVEMENTS_FILE, text: dup },
    ]);
    expect(data.achievements).toHaveLength(1);
  });
});
