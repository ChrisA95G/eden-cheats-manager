<script>
  /** @type {{ html: string }} */
  let { html } = $props();
</script>

<div class="announcement">{@html html}</div>

<style>
  .announcement { overflow-wrap: anywhere; }
  .announcement :global(> * + *) { margin-top: 0.75rem; }
  .announcement :global(ul), .announcement :global(ol) { padding-inline-start: 1.5rem; }
</style>
