// Shared types for the religion panel (Sprint 5.2).

export interface ModRow {
  key: string;
  value: string;
}

/** One unmodeled entry of a religion block (read-only advanced section). */
export interface RawEntry {
  key: string;
  /** "scalar" or "block". */
  kind: string;
  value: string;
}

/** Full religion details payload from the backend `get_religion_details`. */
export interface ReligionDetails {
  key: string;
  group_key: string;
  group_name: string;
  localized_name: string;
  /** Raw ints of `color = { r g b }`, or null. */
  color: [number, number, number] | null;
  /** 1-based icon index (strip frame = icon - 1), or null. */
  icon: number | null;
  country_modifiers: ModRow[];
  province_modifiers: ModRow[];
  heretics: string[];
  enable_date: string | null;
  features: string[];
  raw_remainder: RawEntry[];
  source_file: string;
  raw_block_text: string;
  country_count: number;
  province_count: number;
  sample_tags: string[];
  sample_provinces: number[];
}

/** A religion group option (create flow / move-to-group). */
export interface ReligionGroupEntry {
  key: string;
  name: string;
}

/** Known religion-level feature toggles, with human labels. Mirrors the backend
 *  `RELIGION_FEATURES` list; any present feature not here still shows generically. */
export const FEATURE_LABELS: Record<string, string> = {
  hre_religion: "HRE religion",
  hre_heretic_religion: "HRE heretic religion",
  uses_church_power: "Church power (aspects)",
  fervor: "Fervor",
  uses_karma: "Karma",
  uses_piety: "Piety",
  uses_harmony: "Harmony",
  uses_isolationism: "Isolationism",
  personal_deity: "Personal deity",
  misguided_heretic: "Misguided heretic",
  declare_war_in_regency: "Declare war in regency",
  has_patriarchs: "Patriarch authority",
  allow_female_defenders_of_the_faith: "Female defenders of the faith",
  ancestors: "Ancestor worship",
  authority: "Authority",
  doom: "Doom",
  fetishist_cult: "Fetishist cults",
  religious_reforms: "Religious reforms",
  require_reformed_for_institution_development: "Require reformed for institutions",
  can_have_secondary_religion: "Secondary religion",
  uses_anglican_power: "Anglican power",
  uses_hussite_power: "Hussite power",
  uses_judaism_power: "Judaism power",
};

export const FEATURE_KEYS = Object.keys(FEATURE_LABELS);

/** Pretty label for a feature key (falls back to a prettified key). */
export function featureLabel(key: string): string {
  return (
    FEATURE_LABELS[key] ??
    key.replace(/_/g, " ").replace(/\b\w/g, (c) => c.toUpperCase())
  );
}
