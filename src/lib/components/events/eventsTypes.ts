// Wire types for the Sprint 16 events editor, mirroring the backend `events.rs`
// serde shapes (camelCase over IPC).

/** One `option = { … }` inside an event (mirrors backend `EventOption`). */
export interface EventOption {
  /** The `name = "<loc key>"` value (a loc key). */
  nameKey: string | null;
  /** Resolved localisation for `nameKey`. */
  nameLoc: string | null;
  /** Byte-surgical path to this option block. */
  path: string[];
}

/** One event (mirrors backend `EventEntry`). */
export interface EventEntry {
  /** Full id (`flavor_fra.9100`). */
  id: string;
  /** Namespace portion (`flavor_fra`). */
  namespace: string;
  /** Numeric portion (`9100`), if it parses. */
  number: number | null;
  /** Game-relative file the event was found in. */
  file: string;
  origin: "base" | "mod";
  kind: "country" | "province";
  isTriggeredOnly: boolean;
  fireOnlyOnce: boolean;
  hidden: boolean;
  major: boolean;
  /** The `title` loc key. */
  titleKey: string | null;
  /** The `desc` loc key. */
  descKey: string | null;
  /** Loc-resolved title (for the list). */
  title: string;
  /** Raw loc value for `titleKey` if defined. */
  titleLoc: string | null;
  /** Raw loc value for `descKey` if defined. */
  descLoc: string | null;
  /** `picture` sprite name, if present. */
  picture: string | null;
  /** MTTH base unit (`months`/`years`/`days`), if present. */
  mtthBaseUnit: string | null;
  /** MTTH base value (as written). */
  mtthBaseValue: string | null;
  /** Number of `modifier = { … }` rows in the MTTH block. */
  mtthModifierCount: number;
  /** The options (name loc key + path each). */
  options: EventOption[];
  /** Byte-surgical path to the event block. */
  path: string[];
  triggerPath: string[];
  mtthPath: string[];
  hasTrigger: boolean;
  hasMtth: boolean;
  /** Frontend-only: true when this event is a pending (unsaved) scaffold, so the
   *  "can happen to" list (which reads the saved file) is deferred until Save. */
  pending?: boolean;
}

/** One country's verdict (mirrors backend `CountryVerdict`). */
export interface CountryVerdict {
  tag: string;
  verdict: "yes" | "no" | "unknown";
}

/** Event trigger evaluation (mirrors backend `TriggerEvaluation`). */
export interface EventEvaluation {
  verdicts: CountryVerdict[];
  /** Condition keys the evaluator could not decide (approximate badge). */
  unevaluated: string[];
}

/** One call site firing an event by id (mirrors backend `EventReference`). */
export interface EventReference {
  file: string;
  kind: "country" | "province";
  origin: "base" | "mod";
  location: "events" | "decisions" | "missions";
}

/** The toolkit-owned file brand-new events are scaffolded into. */
export const SCAFFOLD_FILE = "events/zz_eutoolkit_events.txt";

/** The default namespace suggested for a brand-new event. */
export const DEFAULT_NAMESPACE = "eutoolkit";

/** Event pictures are sprites whose name ends in `_eventPicture` (vanilla) —
 *  used as a "contains" filter for the sprite picker (the picker's server-side
 *  prefix filter can't express a suffix, so events filter client-side). */
export const EVENT_PICTURE_SUFFIX = "_eventPicture";
