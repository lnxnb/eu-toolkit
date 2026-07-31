export type View =
  | { kind: "country"; tag: string; tab?: "overview" | "rulers" | "ideas" | "diplomacy" | "estates" | "history" | "names" }
  | { kind: "province"; id: number; tab?: "overview" | "economy" | "military" | "monuments" | "history" | "advanced" }
  | { kind: "religion"; key: string }
  | { kind: "culture"; key: string }
  | { kind: "trade-node"; key: string }
  | { kind: "area"; key: string }
  | { kind: "region"; key: string }
  | { kind: "colonial"; colonialKind: "colonial_regions" | "trade_companies"; key: string }
  | { kind: "adjacency"; index: number }
  | { kind: "climate"; key?: string }
  | { kind: "decisions"; focusKey?: string }
  | { kind: "events"; focusKey?: string }
  | { kind: "missions"; tag?: string }
  | { kind: "government-names"; focusKey?: string }
  | { kind: "estates"; focusKey?: string }
  | { kind: "rebels"; focusKey?: string }
  | { kind: "mechanics"; family?: string; focusKey?: string }
  | { kind: "color-pools" }
  | { kind: "empires"; focusKey?: string }
  | { kind: "technology" }
  | { kind: "scripted"; focusKey?: string }
  | { kind: "on-actions"; focusKey?: string }
  | { kind: "localisation" }
  | { kind: "defines" }
  | { kind: "problems" }
  | { kind: "search" }
  | { kind: "project-changes" }
  | { kind: "edits" }
  | { kind: "new-tab" }
  | { kind: "shortcuts" };

export type ViewKind = View["kind"];
export type DockPreference = "floating" | "docked-right" | "docked-left";

export interface ViewDefinition {
  label: string;
  title: (view: View) => string;
  defaultSize: { w: number; h: number };
  minSize: { w: number; h: number };
  dock: DockPreference;
  singleton?: boolean;
  fullBleed?: boolean;
  paramsEqual: (a: View, b: View) => boolean;
}

const same = (a: View, b: View) => JSON.stringify(a) === JSON.stringify(b);
const singleton = (label: string, size = { w: 900, h: 650 }): ViewDefinition => ({
  label,
  title: () => label,
  defaultSize: size,
  minSize: { w: 420, h: 300 },
  dock: "floating",
  singleton: true,
  paramsEqual: same,
});
const entity = (
  label: string,
  title: (view: View) => string,
  dock: DockPreference = "docked-right",
): ViewDefinition => ({
  label,
  title,
  defaultSize: { w: 360, h: 680 },
  minSize: { w: 320, h: 240 },
  dock,
  paramsEqual: same,
});

/** The single catalog for every surface. Components migrate onto it incrementally. */
export const VIEW_REGISTRY: Record<ViewKind, ViewDefinition> = {
  country: entity("Country", (v) => v.kind === "country" ? v.tag : "Country"),
  province: entity("Province", (v) => v.kind === "province" ? `Province #${v.id}` : "Province"),
  religion: entity("Religion", (v) => v.kind === "religion" ? v.key : "Religion"),
  culture: entity("Culture", (v) => v.kind === "culture" ? v.key : "Culture"),
  "trade-node": entity("Trade node", (v) => v.kind === "trade-node" ? v.key : "Trade node"),
  area: entity("Area", (v) => v.kind === "area" ? v.key : "Area"),
  region: entity("Region", (v) => v.kind === "region" ? v.key : "Region"),
  colonial: entity("Colonial region", (v) => v.kind === "colonial" ? v.key : "Colonial region"),
  adjacency: entity("Adjacency", (v) => v.kind === "adjacency" ? `Adjacency ${v.index + 1}` : "Adjacency"),
  climate: entity("Climate", () => "Climate"),
  decisions: singleton("Decisions"),
  events: singleton("Events"),
  missions: singleton("Missions"),
  "government-names": singleton("Government names"),
  estates: singleton("Estates", { w: 920, h: 680 }),
  rebels: singleton("Rebels"),
  mechanics: singleton("Mechanics"),
  "color-pools": singleton("Color pools"),
  empires: singleton("Empires"),
  technology: singleton("Technology"),
  scripted: singleton("Scripted definitions"),
  "on-actions": singleton("On actions"),
  localisation: singleton("Localisation"),
  defines: singleton("Defines"),
  problems: singleton("Problems", { w: 800, h: 600 }),
  search: singleton("Search", { w: 760, h: 560 }),
  "project-changes": singleton("Project changes"),
  edits: { ...singleton("Edits", { w: 380, h: 680 }), dock: "docked-left" },
  "new-tab": { ...singleton("New tab", { w: 680, h: 520 }), singleton: false },
  shortcuts: singleton("Keyboard shortcuts", { w: 620, h: 520 }),
};

/**
 * `extras` are pre-parameterized entry points into a view that has no dedicated
 * kind — Ideas is the Mechanics view pinned to the `idea_groups` family, the
 * same thing the Tools menu opens.
 */
export const VIEW_GROUPS: { label: string; kinds: ViewKind[]; extras?: { label: string; view: View }[] }[] = [
  { label: "Map entities", kinds: ["country", "province", "religion", "culture", "trade-node", "area", "region", "colonial", "adjacency", "climate"] },
  {
    label: "Game systems",
    kinds: ["decisions", "events", "missions", "government-names", "estates", "rebels", "mechanics", "color-pools", "empires", "technology"],
    extras: [{ label: "Ideas", view: { kind: "mechanics", family: "idea_groups" } }],
  },
  { label: "Scripting", kinds: ["scripted", "on-actions", "localisation", "defines"] },
  { label: "Workspace", kinds: ["edits", "problems", "search", "project-changes", "shortcuts"] },
];

export function viewDefinition(view: View): ViewDefinition {
  return VIEW_REGISTRY[view.kind];
}

export function isView(value: unknown): value is View {
  if (!value || typeof value !== "object") return false;
  const kind = (value as { kind?: unknown }).kind;
  return typeof kind === "string" && kind in VIEW_REGISTRY;
}
