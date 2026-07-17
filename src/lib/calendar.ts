// Sprint 12.1/12.2 — calendar rendering for the date selector.
//
// EU4 dates are "Y.M.D" (year 1-9999, month 1-12, day). Mods reskin the calendar
// two ways, both pure loc: month names (`January`…`December`, e.g. Anbennar's
// "Castanmark(1)") and an era/year template (`WORLD_YEAR`, e.g. Imperium
// Universalis's "The world $YEAR$ AUC"). These helpers turn a raw game date plus
// the mod's resolved calendar strings into the label the chip shows. They are
// pure (no Svelte, no IPC) so they read like unit tests by inspection.

export interface Ymd {
  y: number;
  m: number;
  d: number;
}

/**
 * The 12 month loc keys in order (mirrors the backend `loc::MONTH_KEYS`). These
 * are the plain loc keys the game and mods localize (e.g. Anbennar's
 * "Castanmark(1)"); a month-name edit in the calendar editor writes a
 * `locOverride` under the matching key. There is NO separate month-formatting
 * key family in EU4 — month display resolves these keys directly.
 */
export const MONTH_KEYS = [
  "January",
  "February",
  "March",
  "April",
  "May",
  "June",
  "July",
  "August",
  "September",
  "October",
  "November",
  "December",
] as const;

/**
 * `WORLD_YEAR` is the only era/year-display template EU4 ships (audited: no
 * `DATE_*` family exists; the year label is this one key). Kept as a named
 * constant so the calendar editor and the chip reference the same key.
 */
export const WORLD_YEAR_KEY = "WORLD_YEAR";

/** The mod's resolved calendar for rendering dates: month names + era suffix. */
export interface Calendar {
  /** Resolved January..December (a short/missing entry falls back to numeric). */
  months: string[];
  /** Era suffix from `WORLD_YEAR` (e.g. "AD", "AUC"), or null when undefined. */
  era: string | null;
}

/** Parses "Y.M.D"; non-numeric parts fall back to 1 so the result is total. */
export function parseYmd(s: string): Ymd {
  const [y, m, d] = (s ?? "").split(".").map((p) => parseInt(p, 10));
  return {
    y: Number.isFinite(y) ? y : 1,
    m: Number.isFinite(m) ? m : 1,
    d: Number.isFinite(d) ? d : 1,
  };
}

/** Ordinal for comparing two "Y.M.D" dates (day + 100*month + 10000*year). */
export function ordDate(s: string): number {
  const { y, m, d } = parseYmd(s);
  return y * 10000 + m * 100 + d;
}

/** -1 / 0 / 1 comparison of two "Y.M.D" dates. */
export function compareDates(a: string, b: string): number {
  const x = ordDate(a);
  const y = ordDate(b);
  return x < y ? -1 : x > y ? 1 : 0;
}

/**
 * The era suffix a `WORLD_YEAR` template implies, e.g. "The world $YEAR$ AD" →
 * "AD", "The world $YEAR$ AUC" → "AUC". Returns null when the template is
 * absent, has no `$YEAR$`, or nothing follows the year token. The text after
 * `$YEAR$` is trimmed; a leading punctuation-only remainder yields null.
 */
export function eraSuffix(worldYear: string | null | undefined): string | null {
  if (!worldYear) return null;
  const marker = worldYear.indexOf("$YEAR$");
  if (marker < 0) return null;
  const after = worldYear.slice(marker + "$YEAR$".length).trim();
  return after.length > 0 ? after : null;
}

/**
 * A raw "Y.M.D" date rendered with the mod's calendar: `day monthName year` plus
 * the era suffix when one is defined (e.g. "11 November 1444 AD", or with
 * Anbennar months "11 Castanmark(1) 1444"). `months` is the 12 resolved month
 * names in order; a missing/short entry falls back to the numeric month.
 */
export function formatGameDate(
  date: string,
  months: string[],
  era: string | null,
): string {
  const { y, m, d } = parseYmd(date);
  const monthName = months[m - 1] ?? String(m);
  const base = `${d} ${monthName} ${y}`;
  return era ? `${base} ${era}` : base;
}

/**
 * A raw "Y.M.D" date rendered with a [`Calendar`] — the shared entry point every
 * toolkit-rendered date (chip, timeline headers, diplomacy ranges) goes through
 * so a mod's custom month names + era show everywhere. `cal` may be null/partial
 * before the calendar context loads; it then degrades to numeric months.
 */
export function formatDate(date: string, cal: Calendar | null | undefined): string {
  return formatGameDate(date, cal?.months ?? [], cal?.era ?? null);
}

/** Just the era-suffixed year, for compact contexts ("1444 AD"). */
export function formatGameYear(date: string, era: string | null): string {
  const { y } = parseYmd(date);
  return era ? `${y} ${era}` : String(y);
}

/**
 * The effective start date, mirroring the backend rule (`bookmarks::
 * effective_start_date`): the default bookmark's date, else the earliest
 * bookmark's date, else the vanilla "1444.11.11". `bookmarks` need not be
 * sorted — the earliest is picked by date ordinal here.
 */
export function effectiveStartDate(
  bookmarks: { date: string; isDefault: boolean }[],
): string {
  const dated = bookmarks.filter((b) => b.date && Number.isFinite(ordDate(b.date)));
  const def = dated.filter((b) => b.isDefault);
  const pool = def.length > 0 ? def : dated;
  if (pool.length === 0) return "1444.11.11";
  return pool.reduce((best, b) => (ordDate(b.date) < ordDate(best.date) ? b : best)).date;
}
