import { describe, it, expect } from "vitest";
import { routeForFile, provinceIdOf, countryTagOf } from "./searchRoute";

describe("searchRoute path→editor mapping", () => {
  const mech = new Map<string, string>([
    ["common/disasters", "disasters"],
    ["common/rebel_types", "rebels"], // (overlay wins over mechanics dir below)
  ]);

  it("routes history files to province / country", () => {
    expect(provinceIdOf("history/provinces/1234 - Stockholm.txt")).toBe(1234);
    expect(routeForFile("history/provinces/1234 - Stockholm.txt")).toEqual({ kind: "province", id: 1234 });
    expect(countryTagOf("history/countries/SWE - Sweden.txt")).toBe("SWE");
    expect(routeForFile("history/countries/SWE - Sweden.txt")).toEqual({ kind: "country", tag: "SWE" });
  });

  it("routes common dirs to modes", () => {
    expect(routeForFile("common/religions/00_religion.txt")).toEqual({ kind: "mode", mode: "religion" });
    expect(routeForFile("common/cultures/00_cultures.txt")).toEqual({ kind: "mode", mode: "culture" });
    expect(routeForFile("common/colonial_regions/00.txt")).toEqual({ kind: "mode", mode: "colonial_regions" });
  });

  it("routes common dirs to overlays", () => {
    expect(routeForFile("common/estates/estate_nobles.txt")).toEqual({ kind: "overlay", overlay: "estates" });
    expect(routeForFile("common/rebel_types/anti_tax.txt")).toEqual({ kind: "overlay", overlay: "rebels" });
    expect(routeForFile("common/technologies/adm.txt")).toEqual({ kind: "overlay", overlay: "technology" });
    expect(routeForFile("common/government_names/00.txt")).toEqual({ kind: "overlay", overlay: "govnames" });
    expect(routeForFile("common/scripted_triggers/x.txt")).toEqual({ kind: "overlay", overlay: "scripted" });
    expect(routeForFile("common/imperial_reforms/hre.txt")).toEqual({ kind: "overlay", overlay: "empires" });
    expect(routeForFile("common/defines.lua")).toEqual({ kind: "overlay", overlay: "defines" });
  });

  it("routes mechanics-family dirs via the dir map", () => {
    expect(routeForFile("common/disasters/great_depression.txt", mech)).toEqual({
      kind: "overlay",
      overlay: "mechanics",
      family: "disasters",
    });
  });

  it("routes top-level content dirs", () => {
    expect(routeForFile("decisions/MyDec.txt")).toEqual({ kind: "overlay", overlay: "decisions" });
    expect(routeForFile("events/MyEvt.txt")).toEqual({ kind: "overlay", overlay: "events" });
    expect(routeForFile("missions/MyMis.txt")).toEqual({ kind: "overlay", overlay: "missions" });
    expect(routeForFile("localisation/replace/x_l_english.yml")).toEqual({ kind: "overlay", overlay: "localisation" });
  });

  it("routes map files to modes", () => {
    expect(routeForFile("map/area.txt")).toEqual({ kind: "mode", mode: "areas" });
    expect(routeForFile("map/region.txt")).toEqual({ kind: "mode", mode: "regions" });
    expect(routeForFile("map/climate.txt")).toEqual({ kind: "mode", mode: "climate" });
    expect(routeForFile("map/terrain.txt")).toEqual({ kind: "mode", mode: "simple_terrain" });
    expect(routeForFile("map/adjacencies.csv")).toEqual({ kind: "mode", mode: "provinces" });
  });

  it("falls back to preview for unmapped files", () => {
    expect(routeForFile("history/diplomacy/hre.txt")).toEqual({ kind: "preview" });
    expect(routeForFile("gfx/interface/foo.gfx")).toEqual({ kind: "preview" });
    expect(routeForFile("common/some_unknown_dir/x.txt")).toEqual({ kind: "preview" });
  });

  it("handles backslash paths", () => {
    expect(routeForFile("history\\provinces\\7 - Foo.txt")).toEqual({ kind: "province", id: 7 });
  });
});
