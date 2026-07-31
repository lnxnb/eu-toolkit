<script lang="ts">let { title, detail = "" }: { title: string; detail?: string } = $props();</script>
<div class="empty-state"><div class="mark">◇</div><strong>{title}</strong>{#if detail}<p>{detail}</p>{/if}</div>
<style>.empty-state{min-height:120px;display:grid;place-content:center;justify-items:center;text-align:center;color:var(--text-2);padding:var(--sp-6)}.mark{font-size:28px;color:var(--text-3)}strong{color:var(--text-1)}p{max-width:42ch;margin:var(--sp-2) 0 0;color:var(--text-3)}</style>
