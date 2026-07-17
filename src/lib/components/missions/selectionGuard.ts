// Pure race-guard for the per-country mission board (Sprint 17 bugfix).
//
// Opening a country's board is async: the click kicks off a per-tag
// `evaluate_series_potential` request, and the board must only be populated by
// the response for the country that is CURRENTLY selected. Two hazards this
// guards against:
//   1. First-click race — the board rendered from an empty/not-yet-loaded map
//      before the evaluation landed (showing a false "receives no mission series").
//   2. Stale response — a slow response for a previously-clicked tag arriving
//      after the user picked a different country must NOT populate the wrong board.
//
// Each selection bumps a monotonic sequence and records the requested tag; a
// response applies only when both still match the latest selection.

export interface Selection {
  /** Monotonic sequence number, bumped on every country selection. */
  seq: number;
  /** The tag the selection is for (null before any selection). */
  tag: string | null;
}

/**
 * Whether a potentials response captured as `req` should populate the board,
 * given the latest selection `cur`. Stale (superseded) or mismatched-tag
 * responses are dropped — mirrors MapView's `renderSeq` guard.
 */
export function shouldApplySelection(req: Selection, cur: Selection): boolean {
  return req.seq === cur.seq && req.tag === cur.tag && req.tag !== null;
}
