<script>
  import { debugLogs, clearDebugLogs } from '../stores/debug.js';

  let open = $state(false);
</script>

<div class="dbg-toggle-wrap">
  <button class="dbg-toggle" onclick={() => open = !open}>
    DBG {open ? '▴' : '▾'}
  </button>
</div>

{#if open}
  <div class="dbg-panel">
    <div class="dbg-header">
      <span>// DEBUG LOG</span>
      <button class="dbg-clear" onclick={clearDebugLogs}>CLEAR</button>
    </div>
    <div class="dbg-body">
      {#each $debugLogs as line}
        <div class="dbg-line">{line}</div>
      {:else}
        <div class="dbg-empty">no logs yet</div>
      {/each}
    </div>
  </div>
{/if}

<style>
  .dbg-toggle-wrap {
    position: fixed;
    bottom: 0;
    right: 0;
    z-index: 9000;
  }

  .dbg-toggle {
    background: #1a1a1a;
    color: #f5a800;
    border: 1px solid #f5a800;
    border-bottom: none;
    border-right: none;
    padding: 0.2rem 0.6rem;
    font-size: 0.7rem;
    letter-spacing: 0.1em;
    cursor: pointer;
    font-family: monospace;
  }

  .dbg-panel {
    position: fixed;
    bottom: 1.6rem;
    right: 0;
    width: min(96vw, 540px);
    max-height: 55vh;
    background: rgba(10, 10, 10, 0.96);
    border: 1px solid #f5a800;
    border-right: none;
    z-index: 8999;
    display: flex;
    flex-direction: column;
    font-family: monospace;
  }

  .dbg-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 0.25rem 0.6rem;
    border-bottom: 1px solid #333;
    color: #f5a800;
    font-size: 0.68rem;
    letter-spacing: 0.1em;
    flex-shrink: 0;
  }

  .dbg-clear {
    background: transparent;
    border: 1px solid #555;
    color: #888;
    font-size: 0.6rem;
    padding: 0.1rem 0.4rem;
    cursor: pointer;
    font-family: monospace;
    letter-spacing: 0.08em;
  }

  .dbg-clear:hover { color: #f5a800; border-color: #f5a800; }

  .dbg-body {
    overflow-y: auto;
    padding: 0.3rem 0.6rem;
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
  }

  .dbg-line {
    font-size: 0.68rem;
    color: #ccc;
    white-space: pre-wrap;
    word-break: break-all;
    line-height: 1.4;
  }

  .dbg-empty {
    font-size: 0.68rem;
    color: #555;
    font-style: italic;
  }
</style>
