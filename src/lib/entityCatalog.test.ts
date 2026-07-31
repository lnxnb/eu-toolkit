import { describe, expect, it } from "vitest";
import { hasEntityBrowser, matchesEntity, requiredMapMode, type EntityOption } from "./entityCatalog";
import { VIEW_GROUPS } from "./views";

const option = (over: Partial<EntityOption> = {}): EntityOption => ({
  id: "SWE",
  label: "Sweden",
  hint: "Scandinavia",
  view: { kind: "country", tag: "SWE" },
  ...over,
});

describe("entityCatalog", () => {
  it("gives every Map-entities view a browser, since none can open unparameterized", () => {
    const mapEntities = VIEW_GROUPS.find((g) => g.label === "Map entities")!;
    for (const kind of mapEntities.kinds) expect(hasEntityBrowser(kind), kind).toBe(true);
  });

  it("leaves singleton views to launch directly", () => {
    expect(hasEntityBrowser("decisions")).toBe(false);
    expect(hasEntityBrowser("technology")).toBe(false);
  });

  it("matches on label, id and hint, case-insensitively", () => {
    expect(matchesEntity(option(), "swe")).toBe(true);
    expect(matchesEntity(option(), "SWE")).toBe(true);
    expect(matchesEntity(option(), "scandi")).toBe(true);
    expect(matchesEntity(option(), "denmark")).toBe(false);
  });

  it("treats an empty needle as matching everything", () => {
    expect(matchesEntity(option(), "")).toBe(true);
  });

  it("finds a province by its id", () => {
    const prov = option({ id: "151", label: "Stockholm", hint: "#151", view: { kind: "province", id: 151 } });
    expect(matchesEntity(prov, "151")).toBe(true);
  });

  it("tolerates a missing hint", () => {
    expect(matchesEntity(option({ hint: undefined }), "scandi")).toBe(false);
    expect(matchesEntity(option({ hint: undefined }), "swe")).toBe(true);
  });

  describe("requiredMapMode", () => {
    it("returns null for the self-loading panels", () => {
      expect(requiredMapMode({ kind: "country", tag: "SWE" })).toBeNull();
      expect(requiredMapMode({ kind: "province", id: 1 })).toBeNull();
      expect(requiredMapMode({ kind: "religion", key: "catholic" })).toBeNull();
      expect(requiredMapMode({ kind: "culture", key: "swedish" })).toBeNull();
      expect(requiredMapMode({ kind: "technology" })).toBeNull();
    });

    it("names the mode each mode-scoped panel reads its data from", () => {
      expect(requiredMapMode({ kind: "trade-node", key: "lubeck" })).toBe("trade_nodes");
      expect(requiredMapMode({ kind: "area", key: "sweden_area" })).toBe("areas");
      expect(requiredMapMode({ kind: "region", key: "scandinavia_region" })).toBe("regions");
      expect(requiredMapMode({ kind: "adjacency", index: 0 })).toBe("provinces");
    });

    it("routes each colonial kind to its own mode", () => {
      expect(requiredMapMode({ kind: "colonial", colonialKind: "colonial_regions", key: "x" })).toBe("colonial_regions");
      expect(requiredMapMode({ kind: "colonial", colonialKind: "trade_companies", key: "x" })).toBe("trade_companies");
    });

    it("maps the two climate slots onto their separate modes", () => {
      expect(requiredMapMode({ kind: "climate", key: "winter" })).toBe("winter");
      expect(requiredMapMode({ kind: "climate", key: "climate" })).toBe("climate");
      expect(requiredMapMode({ kind: "climate" })).toBe("climate");
    });
  });
});
