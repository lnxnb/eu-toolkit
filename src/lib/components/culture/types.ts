// Shared types for the culture panel (Sprint 6.2).

/** One unmodeled entry of a culture block (read-only advanced section). */
export interface RawEntry {
  key: string;
  /** "scalar" or "block". */
  kind: string;
  value: string;
}

/** Full culture details payload from the backend `get_culture_details`. */
export interface CultureDetails {
  key: string;
  group_key: string;
  group_name: string;
  localized_name: string;
  /** `primary = TAG` (optional). */
  primary: string | null;
  /** Culture-level name pools (empty when the culture falls back to the group). */
  male_names: string[];
  female_names: string[];
  dynasty_names: string[];
  /** Whether each culture-level pool block exists on disk (empty vs absent). */
  male_names_present: boolean;
  female_names_present: boolean;
  dynasty_names_present: boolean;
  /** Group-level fallback pools. */
  group_male_names: string[];
  group_female_names: string[];
  group_dynasty_names: string[];
  group_male_names_present: boolean;
  group_female_names_present: boolean;
  group_dynasty_names_present: boolean;
  group_graphical_culture: string | null;
  group_second_graphical_culture: string | null;
  raw_remainder: RawEntry[];
  source_file: string;
  raw_block_text: string;
  primary_count: number;
  primary_tags: string[];
  accepted_count: number;
  accepted_tags: string[];
  province_count: number;
  sample_provinces: number[];
}

/** A culture group option (create flow / move-to-group). */
export interface CultureGroupEntry {
  key: string;
  name: string;
}

/** Formats a name-pool list into the inner tokens of a `{ ... }` block: entries
 *  with whitespace are quoted (dynasty names like `von Klinckow`), others bare.
 *  Matches the game's own convention and stays byte-clean through the writer. */
export function poolBlockValue(names: string[]): string {
  return names.map((n) => (/\s/.test(n) ? `"${n}"` : n)).join(" ");
}
