// Pure redistribution math for SliderGroup (normalized sum-to-total sliders with
// per-row lock pins). Kept dependency-free and side-effect-free so it can be unit
// tested in isolation. Consumers: trade-good chances (SPRINT 7.5) and dev mix (9.2).
//
// Invariants (all guaranteed by `redistribute`, verified in sliderMath.test.mjs):
//   1. sum(result) === total (within float epsilon), always.
//   2. Locked rows never move: result[j] === values[j] for every locked j.
//   3. The dragged row equals the requested value, after clamping to the pool that
//      the unlocked rows can actually give up ([0, total - lockedSum]).
//   4. Degenerate case — every *other* row locked (no unlocked row to absorb the
//      change) — is a no-op: result === values unchanged.

/**
 * Set row `index` toward `requested`, keeping `sum === total` by pulling the
 * difference proportionally from the other *unlocked* rows.
 *
 * @param values   current values (must already sum to `total`)
 * @param index    the row being dragged
 * @param requested the desired new value for that row
 * @param locks    per-row lock flags (locked rows are frozen); defaults to all-unlocked
 * @param total    the fixed sum the group must preserve (e.g. 100 or 1)
 * @returns a new array (never mutates `values`)
 */
export function redistribute(
  values: number[],
  index: number,
  requested: number,
  locks: boolean[] = [],
  total = 100,
): number[] {
  const n = values.length;
  const isLocked = (i: number) => locks[i] === true;

  // A locked (or out-of-range) row can't be dragged at all.
  if (index < 0 || index >= n || isLocked(index)) {
    return values.slice();
  }

  // Sum frozen by locks (excluding the dragged row, which is unlocked).
  let lockedSum = 0;
  const others: number[] = [];
  for (let i = 0; i < n; i++) {
    if (i === index) continue;
    if (isLocked(i)) lockedSum += values[i];
    else others.push(i);
  }

  // The pool shared between the dragged row and the unlocked others.
  const pool = total - lockedSum;

  // Nothing else can move → the dragged row is pinned at whatever the pool leaves.
  // (Invariant 4: no unlocked others ⇒ no-op.)
  if (others.length === 0) {
    return values.slice();
  }

  // Clamp the request into what the unlocked rows can actually accommodate.
  const target = Math.min(Math.max(requested, 0), pool);

  const result = values.slice();
  result[index] = target;

  const remaining = pool - target; // must be spread across `others`, all ≥ 0
  let othersSum = 0;
  for (const j of others) othersSum += values[j];

  if (othersSum > 1e-12) {
    // Proportional to current values: preserves relative mix among the others.
    for (const j of others) {
      result[j] = (values[j] / othersSum) * remaining;
    }
  } else {
    // All others are currently 0 → nothing to be proportional to; spread evenly.
    const share = remaining / others.length;
    for (const j of others) result[j] = share;
  }

  return result;
}

/**
 * Round a sum-to-`total` distribution to integers whose sum is exactly `total`
 * using the largest-remainder method (the honest rounding SPRINT 9 wants for dev).
 * Purely for display/commit; the live drag math above stays continuous.
 */
export function roundToTotal(values: number[], total = 100): number[] {
  const floors = values.map((v) => Math.floor(v));
  let used = floors.reduce((a, b) => a + b, 0);
  let leftover = Math.round(total - used);

  // Hand out the remaining units to the largest fractional parts first.
  const order = values
    .map((v, i) => ({ i, frac: v - Math.floor(v) }))
    .sort((a, b) => b.frac - a.frac);

  const result = floors.slice();
  for (let k = 0; k < order.length && leftover > 0; k++) {
    result[order[k].i] += 1;
    leftover--;
  }
  return result;
}
