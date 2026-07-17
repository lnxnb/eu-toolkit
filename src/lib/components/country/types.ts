// Shared types for the country panel sections (Sprint 1.2).

export interface IdeaEffect {
  key: string;
  value: string;
}

/** A leader sub-block on a character (note the game's spelling "manuever"). */
export interface LeaderInfo {
  fire: number | null;
  shock: number | null;
  manuever: number | null;
  siege: number | null;
}

/** One add_*_personality effect + the dated block it lives in (for removal). */
export interface Personality {
  key: string;
  date: string;
}

/** The monarch at 1444. `date` addresses edits at [date, "monarch", …]. */
export interface RulerInfo {
  date: string;
  name: string | null;
  dynasty: string | null;
  adm: number | null;
  dip: number | null;
  mil: number | null;
  birth_date: string | null;
  female: boolean;
  regent: boolean;
  culture: string | null;
  religion: string | null;
  personalities: Personality[];
  leader: LeaderInfo | null;
}

export interface QueenInfo {
  date: string;
  name: string | null;
  dynasty: string | null;
  adm: number | null;
  dip: number | null;
  mil: number | null;
  birth_date: string | null;
  death_date: string | null;
  female: boolean;
  regent: boolean;
  country_of_origin: string | null;
  culture: string | null;
  religion: string | null;
  personalities: Personality[];
  leader: LeaderInfo | null;
}

export interface HeirInfo {
  date: string;
  name: string | null;
  monarch_name: string | null;
  dynasty: string | null;
  adm: number | null;
  dip: number | null;
  mil: number | null;
  birth_date: string | null;
  death_date: string | null;
  claim: number | null;
  female: boolean;
  culture: string | null;
  religion: string | null;
  personalities: Personality[];
  leader: LeaderInfo | null;
}

export interface MonarchName {
  name: string;
  weight: string;
}

export interface NamePools {
  monarch_names: MonarchName[];
  leader_names: string[];
  ship_names: string[];
  army_names: string[];
  fleet_names: string[];
}

/** A pickable idea group (category ADM/DIP/MIL) for historical setup. */
export interface IdeaGroupEntry {
  key: string;
  name: string;
  category: string;
}

/** Full country details payload from the backend `get_country_details`. */
export interface CountryDetails {
  tag: string;
  name: string;
  localized_name: string;
  adjective: string | null;
  color: [number, number, number] | null;
  /** Game-relative common/countries file (color, revo colors, graphical_culture). */
  country_file: string | null;
  /** Game-relative history/countries file (government, culture, capital, …). */
  history_file: string | null;
  /** Three flag-palette indices (NOT 0-255 RGB), or null if absent. */
  revolutionary_colors: [number, number, number] | null;
  graphical_culture: string | null;
  government: string | null;
  government_name: string | null;
  government_rank: number | null;
  religion: string | null;
  religion_name: string | null;
  primary_culture: string | null;
  primary_culture_name: string | null;
  technology_group: string | null;
  technology_group_name: string | null;
  unit_type: string | null;
  national_focus: string | null;
  mercantilism: number | null;
  elector: boolean;
  government_reforms: string[];
  accepted_cultures: string[];
  historical_rivals: string[];
  historical_friends: string[];
  capital: number | null;
  capital_name: string | null;
  ruler: RulerInfo | null;
  ruler_reason: string | null;
  queen: QueenInfo | null;
  heir: HeirInfo | null;
  name_pools: NamePools;
  historical_idea_groups: string[];
  historical_units: string[];
  ideas: {
    name: string;
    localized_name: string;
    traditions_name: string;
    ambition_name: string;
    source_file: string;
    traditions: IdeaEffect[];
    ideas: {
      name: string;
      localized_name: string;
      localized_desc: string;
      effects: IdeaEffect[];
    }[];
    ambition: IdeaEffect[];
  } | null;
  /** Every dated `Y.M.D = { … }` block of the history file, in file order (S3.2
   *  country history timeline). Mirrors the backend `province_details::DatedBlock`. */
  dated_blocks: import("./history").CountryDatedBlock[];
}

/** A registry entry (governments, reforms, tech groups, graphical cultures). */
export interface RegistryEntry {
  key: string;
  name: string;
  source_file: string;
}

/** A grouped option (religion within group, culture within group). */
export interface GroupedEntry {
  key: string;
  name: string;
  group: string;
  group_name: string;
  color: [number, number, number] | null;
}

/** A country brief for tag pickers. */
export interface CountryBrief {
  tag: string;
  name: string;
  color: [number, number, number] | null;
}

/**
 * Display seed for a just-created country whose files aren't on disk yet
 * (Sprint 4.1). `get_country_details` 404s for a pending tag, so CountryPanel
 * renders a read-only "pending scaffold" view from this instead. Mirrors the
 * created-religion seed pattern.
 */
export interface CountryCreateSeed {
  tag: string;
  name: string;
  adjective: string;
  color: [number, number, number];
  capitalId: number;
}
