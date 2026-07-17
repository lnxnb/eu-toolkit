<script lang="ts">
  let { children } = $props();
</script>

{@render children()}

<style>
  :global(html),
  :global(body) {
    margin: 0;
    height: 100%;
  }

  :global(:root) {
    font-family: Inter, Avenir, Helvetica, Arial, sans-serif;
    font-size: 16px;
    line-height: 1.5;
    color: #0f0f0f;
    background-color: #f6f6f6;
    -webkit-font-smoothing: antialiased;
  }

  @media (prefers-color-scheme: dark) {
    :global(:root) {
      color: #f6f6f6;
      background-color: #2f2f2f;
    }
  }
</style>
