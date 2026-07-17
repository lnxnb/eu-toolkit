// Wire types for the Sprint 17 mission-tree designer, mirroring the backend
// `missions.rs` serde shapes (camelCase over IPC).

/** One mission node inside a series (mirrors backend `MissionEntry`). */
export interface MissionEntry {
  /** The mission script key. */
  key: string;
  /** The `icon` sprite name (bare `mission_*` GFX name), if present. */
  icon: string | null;
  /** Explicit `position` (row), if written. */
  position: number | null;
  /** 1-based index within the series (the absent-position default). */
  ordinal: number;
  /** `position` when present, else `ordinal` — the row the board lays it at. */
  effectivePosition: number;
  /** Prerequisite mission keys (bare tokens from `required_missions`). */
  requiredMissions: string[];
  /** The `completed_by` date scalar, if present. */
  completedBy: string | null;
  /** Loc-resolved title (`<key>_title`, else the prettified key). */
  title: string;
  titleKey: string;
  descKey: string;
  titleLoc: string | null;
  descLoc: string | null;
  /** Byte-surgical path to the mission block (`["<series>", "<mission>"]`). */
  path: string[];
  triggerPath: string[];
  effectPath: string[];
  provincesPath: string[];
  requiredPath: string[];
  hasTrigger: boolean;
  hasEffect: boolean;
  hasProvinces: boolean;
  /** Whether a `required_missions = { … }` block already exists (AddId guard). */
  hasRequiredBlock: boolean;
  /** Frontend-only: set on a pending (unsaved) scaffolded mission node. */
  pendingBadge?: boolean;
}

/** One mission series (mirrors backend `MissionSeries`). */
export interface MissionSeries {
  key: string;
  file: string;
  origin: "base" | "mod";
  slot: number | null;
  generic: boolean;
  ai: boolean;
  hasCountryShield: boolean;
  hasPotential: boolean;
  path: string[];
  potentialPath: string[];
  missions: MissionEntry[];
  /** Frontend-only: set on a pending (unsaved) scaffolded series. */
  pending?: boolean;
  /** Frontend-only: the TAG a pending series was scaffolded for (tag-gated). */
  pendingTag?: string;
  /** Frontend-only: shown only because the user expanded the approximate set. */
  approx?: boolean;
}

/** One series' batched `potential` verdict (mirrors backend `SeriesPotential`). */
export interface SeriesPotential {
  key: string;
  file: string;
  yes: string[];
  unknown: string[];
  unevaluated: string[];
}

/** The toolkit-owned file brand-new series are scaffolded into. */
export const SCAFFOLD_FILE = "missions/zz_eutoolkit_missions.txt";

/** Mission icons are sprites whose name starts with `mission_` (the icon value in
 *  a mission block is the sprite NAME verbatim, not a `GFX_`-prefixed alias). */
export const MISSION_ICON_PREFIX = "mission_";

/** A stable identity for a series across the fetched + pending lists. */
export function seriesId(s: MissionSeries): string {
  return `${s.file}::${s.key}`;
}
