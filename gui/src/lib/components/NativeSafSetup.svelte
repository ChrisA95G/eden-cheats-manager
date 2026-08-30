<script>
  import { invoke } from '@tauri-apps/api/core';
  import { onMount, untrack } from 'svelte';

  /**
   * @typedef {Object} EdenLoadAccessStatus
   * @property {boolean} selected
   * @property {boolean} validLocation
   * @property {boolean} readPermission
   * @property {boolean} writePermission
   * @property {boolean} readable
   * @property {boolean} writable
   * @property {boolean} ready
   * @property {string} message
   */

  /** @type {{ initialStatus?: any, onready?: function }} */
  let { initialStatus = null, onready } = $props();

  let status = $state(/** @type {EdenLoadAccessStatus | null} */ (
    untrack(() => initialStatus)
  ));
  let checking = $state(false);
  let pickerOpened = $state(false);
  let error = $state('');

  async function refreshStatus() {
    if (checking) return;
    checking = true;
    error = '';
    try {
      const next = /** @type {EdenLoadAccessStatus} */ (
        await invoke('get_eden_load_access_status')
      );
      status = next;
      if (next.ready) {
        await onready?.(next);
      }
    } catch (cause) {
      error = String(cause);
    } finally {
      checking = false;
    }
  }

  async function selectDirectory() {
    error = '';
    pickerOpened = true;
    try {
      await invoke('select_eden_load_directory');
    } catch (cause) {
      pickerOpened = false;
      error = String(cause);
    }
  }

  onMount(() => {
    let timer = 0;
    const refreshAfterResume = () => {
      if (document.visibilityState !== 'visible') return;
      window.clearTimeout(timer);
      timer = window.setTimeout(() => {
        pickerOpened = false;
        refreshStatus();
      }, 250);
    };

    window.addEventListener('focus', refreshAfterResume);
    document.addEventListener('visibilitychange', refreshAfterResume);
    if (!status) refreshStatus();

    return () => {
      window.clearTimeout(timer);
      window.removeEventListener('focus', refreshAfterResume);
      document.removeEventListener('visibilitychange', refreshAfterResume);
    };
  });
</script>

<main class="setup-shell">
  <section class="setup-card" aria-labelledby="native-setup-title">
    <div class="brand-row">
      <span class="brand">ECM</span>
      <span class="platform">ANDROID</span>
    </div>

    <div class="copy">
      <p class="eyebrow">// STORAGE SETUP</p>
      <h1 id="native-setup-title">Connect Eden's load directory</h1>
      <p>
        ECM needs access to <strong>Eden → load</strong> to install and remove cheats.
        Android will open Eden like a storage provider.
      </p>
    </div>

    <ol class="steps">
      <li>Tap <strong>Select directory</strong>.</li>
      <li>Choose <strong>Eden</strong> from the storage-provider menu.</li>
      <li>Open <strong>load</strong>, then tap <strong>Use this folder</strong>.</li>
    </ol>

    <div class="status-panel" aria-live="polite">
      <div class:ok={status?.validLocation} class="status-row">
        <span>{status?.validLocation ? '[OK]' : '[--]'}</span>
        <span>Exact Eden load directory</span>
      </div>
      <div class:ok={status?.readPermission && status?.writePermission} class="status-row">
        <span>{status?.readPermission && status?.writePermission ? '[OK]' : '[--]'}</span>
        <span>Persisted read and write permission</span>
      </div>
      <div class:ok={status?.readable && status?.writable} class="status-row">
        <span>{status?.readable && status?.writable ? '[OK]' : '[--]'}</span>
        <span>Eden provider is ready</span>
      </div>

      <p class:ready={status?.ready} class="status-message">
        {#if checking}
          Checking Eden access…
        {:else if error}
          {error}
        {:else}
          {status?.message ?? 'Eden access has not been checked yet.'}
        {/if}
      </p>
    </div>

    <div class="actions">
      <button class="primary" onclick={selectDirectory}>
        [ SELECT EDEN → LOAD ]
      </button>
      <button class="secondary" onclick={refreshStatus} disabled={checking}>
        {checking ? '[ CHECKING… ]' : '[ CHECK AGAIN ]'}
      </button>
    </div>

    {#if pickerOpened}
      <p class="picker-hint">Complete the selection in Android's folder picker.</p>
    {/if}
  </section>
</main>

<style>
  .setup-shell {
    min-height: 100dvh;
    display: grid;
    place-items: center;
    padding: max(1.25rem, env(safe-area-inset-top)) 1rem max(1.25rem, env(safe-area-inset-bottom));
    background:
      radial-gradient(circle at 50% 0%, rgba(var(--accent-rgb), 0.08), transparent 42%),
      var(--bg);
  }

  .setup-card {
    width: min(100%, 34rem);
    border: 1px solid var(--border);
    border-top: 3px solid var(--accent);
    background: var(--surface);
    box-shadow: 0 1.5rem 4rem rgba(0, 0, 0, 0.35);
  }

  .brand-row {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    padding: 1rem 1.25rem;
    border-bottom: 1px solid var(--border);
    letter-spacing: 0.16em;
  }

  .brand { color: var(--accent); font-weight: 700; }
  .platform { color: var(--text-muted); font-size: 0.78rem; }

  .copy { padding: 1.5rem 1.25rem 1rem; }
  .eyebrow { color: var(--accent); font-size: 0.76rem; letter-spacing: 0.14em; }

  h1 {
    margin-top: 0.45rem;
    color: var(--text-bright);
    font-size: clamp(1.25rem, 5vw, 1.75rem);
    line-height: 1.2;
  }

  .copy > p:last-child {
    margin-top: 0.85rem;
    color: var(--text-muted);
    line-height: 1.65;
  }

  .steps {
    margin: 0 1.25rem 1.25rem;
    padding: 1rem 1rem 1rem 2.4rem;
    border: 1px solid var(--border);
    color: var(--text);
    line-height: 1.75;
  }

  .status-panel {
    margin: 0 1.25rem;
    border: 1px solid var(--border);
    background: var(--bg);
  }

  .status-row {
    display: grid;
    grid-template-columns: 3rem 1fr;
    gap: 0.5rem;
    min-height: 2.75rem;
    align-items: center;
    padding: 0.65rem 0.85rem;
    border-bottom: 1px solid var(--border);
    color: var(--text-muted);
  }

  .status-row.ok { color: var(--text-bright); }
  .status-row.ok > span:first-child { color: var(--accent); }

  .status-message {
    min-height: 3.25rem;
    display: flex;
    align-items: center;
    padding: 0.75rem 0.85rem;
    color: var(--error);
    line-height: 1.45;
  }

  .status-message.ready { color: var(--text-bright); }

  .actions {
    display: grid;
    gap: 0.75rem;
    padding: 1.25rem;
  }

  button {
    min-height: 3.25rem;
    padding: 0.8rem 1rem;
    font: inherit;
    letter-spacing: 0.06em;
    cursor: pointer;
  }

  button:disabled { cursor: wait; opacity: 0.55; }

  .primary {
    border: 1px solid var(--accent);
    background: var(--accent);
    color: var(--bg);
    font-weight: 700;
  }

  .secondary {
    border: 1px solid var(--border);
    background: transparent;
    color: var(--text);
  }

  .picker-hint {
    padding: 0 1.25rem 1.25rem;
    color: var(--text-muted);
    text-align: center;
    font-size: 0.8rem;
  }

  @media (min-width: 42rem) {
    .actions { grid-template-columns: 1fr auto; }
  }
</style>
