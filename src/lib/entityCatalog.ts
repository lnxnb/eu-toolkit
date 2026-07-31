// The entity browser behind the New-tab page's "Map entities" group.
//
// Every one of those views is parameterized (a country needs a tag, an area a
// key), so the picker cannot open them from the catalog button alone — it
// drills into a list first. This module is the loader table: one entry per
// parameterized ViewKind saying how to enumerate its universe and how to turn a
// row back into a `View`. Lists come from the same backend commands the panels
// use, so mod content shows up for free.

import { invoke } from "@tauri-apps/api/core";
import type { AdjRow } from "$lib/adjnet";
import type { ClimatePayload } from "$lib/climate";
import type { ColonialData } from "$lib/colonial";
import type { GeoNetwork } from "$lib/geonet";
import type { TradeNetwork } from "$lib/tradenet";
import type { View, ViewKind } from "$lib/views";

/** One pickable row: `label` is searched, `hint` is the muted right-hand note. */
export interface EntityOption {
  id: string;
  label: string;
  hint?: string;
  view: View;
}

export interface EntitySource {
  /** Placeholder for the drill-down search box. */
  searchLabel: string;
  load: (installPath: string, modPath: string | null) => Promise<EntityOption[]>;
}

interface Brief { key: string; name: string }

const ENTITY_SOURCES: Partial<Record<ViewKind, EntitySource>> = {
  country: {
    searchLabel: "Search countries…",
    load: async (installPath, modPath) => {
      const rows = await invoke<{ tag: string; name: string }[]>("list_countries", { installPath, modPath });
      return rows.map((r) => ({ id: r.tag, label: r.name, hint: r.tag, view: { kind: "country", tag: r.tag } }));
    },
  },
  province: {
    searchLabel: "Search provinces by name or id…",
    load: async (installPath, modPath) => {
      const rows = await invoke<{ id: number; name: string }[]>("list_provinces", { installPath, modPath });
      return rows.map((r) => ({ id: String(r.id), label: r.name, hint: `#${r.id}`, view: { kind: "province", id: r.id } }));
    },
  },
  religion: {
    searchLabel: "Search religions…",
    load: async (installPath, modPath) => {
      const rows = await invoke<(Brief & { group_name?: string })[]>("list_religions", { installPath, modPath });
      return rows.map((r) => ({ id: r.key, label: r.name, hint: r.group_name, view: { kind: "religion", key: r.key } }));
    },
  },
  culture: {
    searchLabel: "Search cultures…",
    load: async (installPath, modPath) => {
      const rows = await invoke<(Brief & { group_name?: string })[]>("list_cultures", { installPath, modPath });
      return rows.map((r) => ({ id: r.key, label: r.name, hint: r.group_name, view: { kind: "culture", key: r.key } }));
    },
  },
  "trade-node": {
    searchLabel: "Search trade nodes…",
    load: async (installPath, modPath) => {
      const net = await invoke<TradeNetwork>("get_trade_network", { installPath, modPath });
      return net.nodes.map((n) => ({
        id: n.key,
        label: n.name,
        hint: `${n.members.length} provinces`,
        view: { kind: "trade-node", key: n.key },
      }));
    },
  },
  area: {
    searchLabel: "Search areas…",
    load: async (installPath, modPath) => {
      const net = await invoke<GeoNetwork>("get_geo_network", { installPath, modPath });
      return net.areas.map((a) => ({
        id: a.key,
        label: a.name,
        hint: `${a.provinces.length} provinces`,
        view: { kind: "area", key: a.key },
      }));
    },
  },
  region: {
    searchLabel: "Search regions…",
    load: async (installPath, modPath) => {
      const net = await invoke<GeoNetwork>("get_geo_network", { installPath, modPath });
      return net.regions.map((r) => ({
        id: r.key,
        label: r.name,
        hint: `${r.areas.length} areas`,
        view: { kind: "region", key: r.key },
      }));
    },
  },
  colonial: {
    searchLabel: "Search colonial regions and trade companies…",
    load: async (installPath, modPath) => {
      // Two sibling registries share one view kind; the hint says which.
      const kinds = ["colonial_regions", "trade_companies"] as const;
      const payloads = await Promise.all(kinds.map((kind) => invoke<ColonialData>("get_colonial_data", { kind, installPath, modPath })));
      return payloads.flatMap((data, i) =>
        data.entries.map((e) => ({
          id: `${kinds[i]}:${e.key}`,
          label: e.name,
          hint: kinds[i] === "colonial_regions" ? "Colonial region" : "Trade company",
          view: { kind: "colonial", colonialKind: kinds[i], key: e.key } as View,
        })),
      );
    },
  },
  adjacency: {
    searchLabel: "Search straits and canals…",
    load: async (installPath, modPath) => {
      const payload = await invoke<{ rows: AdjRow[] }>("get_adjacencies", { installPath, modPath });
      return payload.rows.map((row, index) => ({
        id: String(index),
        label: row.comment.trim() || `${row.from} → ${row.to}`,
        hint: row.kind,
        view: { kind: "adjacency", index },
      }));
    },
  },
  climate: {
    // Not a per-entry list: the climate view's `key` selects which of the two
    // independent slots in climate.txt the panel edits.
    searchLabel: "Search climate slots…",
    load: async () => [
      { id: "climate", label: "Climate zones", hint: "tropical · arid · arctic", view: { kind: "climate", key: "climate" } },
      { id: "winter", label: "Winter severity", hint: "mild · normal · severe", view: { kind: "climate", key: "winter" } },
    ],
  },
};

/** Whether this view kind opens a drill-down list rather than launching directly. */
export function hasEntityBrowser(kind: ViewKind): boolean {
  return kind in ENTITY_SOURCES;
}

export function entitySource(kind: ViewKind): EntitySource | null {
  return ENTITY_SOURCES[kind] ?? null;
}

/**
 * The map mode a view needs loaded to render at all. Country/province/religion/
 * culture panels load their own data; the rest read mode-scoped state that only
 * exists once the map is in that mode, so opening them from the picker has to
 * switch modes too.
 */
export function requiredMapMode(view: View): string | null {
  switch (view.kind) {
    case "trade-node": return "trade_nodes";
    case "area": return "areas";
    case "region": return "regions";
    case "colonial": return view.colonialKind;
    case "adjacency": return "provinces";
    case "climate": return view.key === "winter" ? "winter" : "climate";
    default: return null;
  }
}

/** Case-insensitive substring match over label + hint + id. */
export function matchesEntity(option: EntityOption, needle: string): boolean {
  if (!needle) return true;
  const q = needle.toLowerCase();
  return option.label.toLowerCase().includes(q)
    || option.id.toLowerCase().includes(q)
    || (option.hint?.toLowerCase().includes(q) ?? false);
}
