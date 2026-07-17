<!--
  Stepper — a compact integer stepper (−/value/+) with min/max clamps, used by the
  Ruler/Queen/Heir sections for ADM/DIP/MIL pips (0-6), leader stats, and claim.
-->
<script lang="ts">
  let {
    value,
    min = 0,
    max = 6,
    edited = false,
    onchange,
  }: {
    value: number | null;
    min?: number;
    max?: number;
    edited?: boolean;
    onchange: (v: number) => void;
  } = $props();

  const shown = $derived(value ?? 0);

  function set(v: number) {
    const clamped = Math.max(min, Math.min(max, v));
    if (clamped !== value) onchange(clamped);
  }
</script>

<span class="stepper" class:edited>
  <button class="pm" onclick={() => set(shown - 1)} disabled={shown <= min} aria-label="Decrease">
    −
  </button>
  <input
    class="val"
    type="number"
    {min}
    {max}
    value={shown}
    onchange={(e) => set(parseInt(e.currentTarget.value, 10) || 0)}
  />
  <button class="pm" onclick={() => set(shown + 1)} disabled={shown >= max} aria-label="Increase">
    +
  </button>
</span>

<style>
  .stepper {
    display: inline-flex;
    align-items: center;
    gap: 0.15rem;
  }

  .stepper.edited .val {
    border-color: rgba(234, 179, 8, 0.55);
  }

  .pm {
    width: 1.35rem;
    height: 1.35rem;
    border: 1px solid #1f242c;
    background: #3f4855;
    color: #cfd4db;
    font-size: 0.9rem;
    line-height: 1;
    cursor: pointer;
    padding: 0;
  }

  .pm:hover:not(:disabled) {
    background: #4a6da7;
    color: #fff;
  }

  .pm:disabled {
    opacity: 0.4;
    cursor: default;
  }

  .val {
    width: 2.6rem;
    text-align: center;
    background: #21262e;
    border: 1px solid #1f242c;
    color: #cfd4db;
    font-family: inherit;
    font-size: 0.85rem;
    padding: 0.2rem 0.2rem;
    outline: none;
  }
</style>
