<!--
  DatePicker — an EU4 date (year 1-9999, month, day; no time). Three linked numeric
  fields with validation; value round-trips as a "Y.M.D" string in the game's format.
  Consumers: province timeline dated blocks (2.3), diplomacy relation dates (3.3).

  Validation: month clamps to 1-12, day clamps to the days-in-month (leap-aware for
  February), year to 1-9999. Optional min/max ("Y.M.D") clamp the whole date.
-->
<script lang="ts">
  let {
    value = $bindable("1444.11.11"),
    min = undefined,
    max = undefined,
    onchange,
  }: {
    value?: string;
    min?: string | undefined;
    max?: string | undefined;
    onchange?: (v: string) => void;
  } = $props();

  interface YMD {
    y: number;
    m: number;
    d: number;
  }

  function parse(s: string): YMD {
    const parts = s.split(".");
    const y = parseInt(parts[0], 10);
    const m = parseInt(parts[1], 10);
    const d = parseInt(parts[2], 10);
    return {
      y: Number.isFinite(y) ? y : 1,
      m: Number.isFinite(m) ? m : 1,
      d: Number.isFinite(d) ? d : 1,
    };
  }

  function daysInMonth(y: number, m: number): number {
    if (m === 2) {
      const leap = (y % 4 === 0 && y % 100 !== 0) || y % 400 === 0;
      return leap ? 29 : 28;
    }
    return [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31][m - 1] ?? 31;
  }

  function clampInt(n: number, lo: number, hi: number): number {
    if (!Number.isFinite(n)) return lo;
    return Math.max(lo, Math.min(hi, Math.trunc(n)));
  }

  /** Comparable integer for min/max clamping (day + 100*month + 10000*year). */
  function ord(d: YMD): number {
    return d.y * 10000 + d.m * 100 + d.d;
  }

  function normalize(d: YMD): YMD {
    const y = clampInt(d.y, 1, 9999);
    const m = clampInt(d.m, 1, 12);
    const day = clampInt(d.d, 1, daysInMonth(y, m));
    let out = { y, m, d: day };
    if (min) {
      const lo = parse(min);
      if (ord(out) < ord(lo)) out = lo;
    }
    if (max) {
      const hi = parse(max);
      if (ord(out) > ord(hi)) out = hi;
    }
    return out;
  }

  let ymd = $derived(parse(value));

  function set(part: "y" | "m" | "d", raw: string) {
    const next = normalize({ ...ymd, [part]: parseInt(raw, 10) });
    const s = `${next.y}.${next.m}.${next.d}`;
    value = s;
    onchange?.(s);
  }

  const monthNames = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun",
    "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
  ];
</script>

<div class="date-picker">
  <input
    class="year"
    type="number"
    min="1"
    max="9999"
    aria-label="Year"
    value={ymd.y}
    onchange={(e) => set("y", e.currentTarget.value)}
  />
  <span class="sep">.</span>
  <select
    class="month"
    aria-label="Month"
    value={ymd.m}
    onchange={(e) => set("m", e.currentTarget.value)}
  >
    {#each monthNames as name, i}
      <option value={i + 1}>{i + 1} {name}</option>
    {/each}
  </select>
  <span class="sep">.</span>
  <input
    class="day"
    type="number"
    min="1"
    max="31"
    aria-label="Day"
    value={ymd.d}
    onchange={(e) => set("d", e.currentTarget.value)}
  />
</div>

<style>
  .date-picker {
    display: inline-flex;
    align-items: center;
    gap: 0.2rem;
  }

  input,
  select {
    background: #21262e;
    border: 1px solid #1f242c;
    color: #cfd4db;
    font-family: inherit;
    font-size: 0.83rem;
    padding: 0.2rem 0.3rem;
    outline: none;
  }

  .year {
    width: 3.8rem;
  }

  .day {
    width: 3rem;
  }

  .sep {
    color: #8a919c;
  }
</style>
