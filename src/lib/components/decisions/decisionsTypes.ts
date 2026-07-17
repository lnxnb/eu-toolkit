// Wire types for the Sprint 15 decisions editor, mirroring the backend
// `decisions.rs` serde shapes (camelCase over IPC).

/** One `country_decisions` entry (mirrors backend `DecisionEntry`). */
export interface DecisionEntry {
  key: string;
  /** Game-relative file the decision was found in. */
  file: string;
  /** `base` | `mod` — which layer the source file came from. */
  origin: "base" | "mod";
  major: boolean;
  /** Loc-resolved title (for the list). */
  title: string;
  /** `<key>_title` loc key. */
  titleKey: string;
  /** `<key>_desc` loc key. */
  descKey: string;
  /** Raw `<key>_title` loc value if defined. */
  titleLoc: string | null;
  /** Raw `<key>_desc` loc value if defined. */
  descLoc: string | null;
  /** Braces-inclusive `ai_will_do = { … }` raw text, if present. */
  aiWillDo: string | null;
  /** Byte-surgical path to the decision block. */
  path: string[];
  potentialPath: string[];
  allowPath: string[];
  effectPath: string[];
  hasPotential: boolean;
  hasAllow: boolean;
  hasEffect: boolean;
  /** Frontend-only: true when this decision is a pending (unsaved) scaffold, so
   *  the availability list (which reads the saved file) is deferred until Save. */
  pending?: boolean;
}

/** One country's verdict (mirrors backend `CountryVerdict`). */
export interface CountryVerdict {
  tag: string;
  verdict: "yes" | "no" | "unknown";
}

/** Decision availability (mirrors backend `TriggerEvaluation`). */
export interface DecisionAvailability {
  verdicts: CountryVerdict[];
  /** Condition keys the evaluator could not decide (approximate badge). */
  unevaluated: string[];
}

/** The toolkit-owned file brand-new decisions are scaffolded into. */
export const SCAFFOLD_FILE = "decisions/zz_eutoolkit_decisions.txt";
