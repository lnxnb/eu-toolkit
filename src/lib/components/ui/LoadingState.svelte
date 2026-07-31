<script lang="ts">let { label = "Loading…" }: { label?: string } = $props();</script>
<div class="loading-state" role="status" aria-live="polite"><span class="spinner"></span><span>{label}</span></div>
<style>.loading-state{min-height:96px;display:grid;place-content:center;justify-items:center;gap:var(--sp-3);color:var(--text-2)}.spinner{width:22px;height:22px;border:2px solid var(--border);border-top-color:var(--accent);border-radius:50%;animation:spin .8s linear infinite}@keyframes spin{to{transform:rotate(360deg)}}@media(prefers-reduced-motion:reduce){.spinner{animation:none}}</style>
