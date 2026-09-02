<script>
  import { invoke } from '@tauri-apps/api/core';
  import { onMount } from 'svelte';

  /**
   * @typedef {Object} PackageDiscoveryStatus
   * @property {boolean} prodKeysSelected
   * @property {string} prodKeysName
   * @property {boolean} prodKeysReadable
   * @property {boolean} prodKeysSeekable
   * @property {boolean} packageSelected
   * @property {string} packageName
   * @property {boolean} packageReadable
   * @property {boolean} packageSeekable
   * @property {boolean} ready
   * @property {string} message
   */

  /**
   * @typedef {Object} PackageMetadata
   * @property {string} packageFormat
   * @property {string} contentKind
   * @property {string} titleId
   * @property {string} baseTitleId
   * @property {string} programTitleId
   * @property {number} version
   * @property {string} buildId
   * @property {string} moduleId
   * @property {boolean} hasBktr
   * @property {boolean} matchedProgramContentId
   */

  /** @type {{ expectedBaseTitleId?: string, onmetadata?: function, compact?: boolean }} */
  let { expectedBaseTitleId = '', onmetadata, compact = false } = $props();

  let status = $state(/** @type {PackageDiscoveryStatus | null} */ (null));
  let metadata = $state(/** @type {PackageMetadata | null} */ (null));
  let checking = $state(false);
  let discovering = $state(false);
  let pickerOpened = $state(false);
  let error = $state('');

  async function refreshStatus() {
    if (checking) return;
    checking = true;
    try {
      status = /** @type {PackageDiscoveryStatus} */ (
        await invoke('get_package_discovery_status')
      );
    } catch (cause) {
      error = String(cause);
    } finally {
      checking = false;
    }
  }

  /** @param {'keys' | 'package'} kind */
  async function selectDocument(kind) {
    error = '';
    metadata = null;
    pickerOpened = true;
    try {
      await invoke(kind === 'keys'
        ? 'select_prod_keys_document'
        : 'select_game_package_document');
    } catch (cause) {
      pickerOpened = false;
      error = String(cause);
    }
  }

  async function discover() {
    if (!status?.ready || discovering) return;
    discovering = true;
    error = '';
    metadata = null;
    try {
      const result = /** @type {PackageMetadata} */ (
        await invoke('discover_package_metadata')
      );
      const expected = expectedBaseTitleId.trim().toUpperCase();
      if (expected && result.baseTitleId.toUpperCase() !== expected) {
        throw new Error(
          `Package belongs to ${result.baseTitleId}, not selected game ${expected}.`
        );
      }
      metadata = result;
      await onmetadata?.(result);
    } catch (cause) {
      error = String(cause).replace(/^Error:\s*/, '');
    } finally {
      discovering = false;
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
    refreshStatus();

    return () => {
      window.clearTimeout(timer);
      window.removeEventListener('focus', refreshAfterResume);
      document.removeEventListener('visibilitychange', refreshAfterResume);
    };
  });
</script>

<div class="package-discovery" class:compact>
  <p class="intro">
    Read the exact cheat Build ID from a package you own. ECM stores only Android's
    read permission; key contents are never copied into app storage.
  </p>

  <div class="documents">
    <div class:ready={status?.prodKeysReadable && status?.prodKeysSeekable} class="document-row">
      <div>
        <strong>PROD.KEYS</strong>
        <span>{status?.prodKeysName || 'Not selected'}</span>
      </div>
      <button onclick={() => selectDocument('keys')}>[ SELECT KEYS ]</button>
    </div>

    <div class:ready={status?.packageReadable && status?.packageSeekable} class="document-row">
      <div>
        <strong>NSP / XCI</strong>
        <span>{status?.packageName || 'Not selected'}</span>
      </div>
      <button onclick={() => selectDocument('package')}>[ SELECT PACKAGE ]</button>
    </div>
  </div>

  <div class="actions">
    <button class="inspect" onclick={discover} disabled={!status?.ready || discovering}>
      {discovering ? '[ READING PACKAGE… ]' : '[ READ PACKAGE BUILD ID ]'}
    </button>
    <button class="refresh" onclick={refreshStatus} disabled={checking}>
      {checking ? '[ … ]' : '[ REFRESH ]'}
    </button>
  </div>

  {#if pickerOpened}
    <p class="message">Complete the selection in Android's file picker.</p>
  {:else if error}
    <p class="message error">{error}</p>
  {:else if metadata}
    <div class="metadata" aria-live="polite">
      <div><span>Package</span><strong>{metadata.packageFormat} · {metadata.contentKind}</strong></div>
      <div><span>Title ID</span><code>{metadata.titleId}</code></div>
      <div><span>Base ID</span><code>{metadata.baseTitleId}</code></div>
      <div><span>CNMT version</span><strong>{metadata.version}</strong></div>
      <div><span>Build ID</span><code class="build-id">{metadata.buildId}</code></div>
      <p>
        CNMT Program entry matched by Content ID{metadata.hasBktr ? '; BKTR content detected outside the extracted executable.' : '.'}
      </p>
    </div>
  {:else}
    <p class:ready={status?.ready} class="message">
      {status?.message ?? 'Checking selected documents…'}
    </p>
  {/if}
</div>

<style>
  .package-discovery {
    display: grid;
    gap: .7rem;
    color: var(--text-muted);
  }

  .intro {
    margin: 0;
    font-size: .76rem;
    line-height: 1.55;
  }

  .documents {
    display: grid;
    gap: 1px;
    border: 1px solid var(--border);
    background: var(--border);
  }

  .document-row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    align-items: center;
    gap: .75rem;
    min-height: 3.5rem;
    padding: .55rem .65rem;
    background: var(--bg);
  }

  .document-row > div {
    min-width: 0;
    display: grid;
    gap: .2rem;
  }

  .document-row strong {
    color: var(--text-dim);
    font-size: .68rem;
    letter-spacing: .1em;
  }

  .document-row.ready strong { color: var(--accent); }

  .document-row span {
    overflow: hidden;
    color: var(--text);
    font-size: .72rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  button {
    min-height: 2.5rem;
    border: 1px solid var(--border);
    background: transparent;
    color: var(--text-muted);
    padding: .5rem .65rem;
    font: inherit;
    font-size: .68rem;
    letter-spacing: .04em;
    cursor: pointer;
  }

  button:not(:disabled):hover,
  button:not(:disabled):focus-visible {
    border-color: var(--accent);
    color: var(--accent);
  }

  button:disabled { opacity: .45; cursor: default; }

  .actions {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: .5rem;
  }

  .inspect {
    border-color: var(--accent);
    color: var(--accent);
  }

  .message {
    margin: 0;
    min-height: 1.2rem;
    color: var(--text-muted);
    font-size: .72rem;
    line-height: 1.45;
  }

  .message.ready { color: var(--text); }
  .message.error { color: var(--error); }

  .metadata {
    display: grid;
    border-left: 2px solid var(--accent);
    background: var(--bg);
    padding: .65rem .75rem;
    gap: .35rem;
  }

  .metadata > div {
    display: grid;
    grid-template-columns: 7.5rem minmax(0, 1fr);
    gap: .5rem;
    align-items: baseline;
    font-size: .72rem;
  }

  .metadata span { color: var(--text-dim); }
  .metadata strong, .metadata code { color: var(--text); }
  .metadata .build-id { color: var(--accent); font-weight: 700; }

  .metadata p {
    margin: .25rem 0 0;
    color: var(--text-muted);
    font-size: .68rem;
    line-height: 1.45;
  }

  .compact .intro { display: none; }

  @media (max-width: 520px) {
    .document-row {
      grid-template-columns: 1fr;
      gap: .5rem;
    }

    .document-row button { width: 100%; }
    .metadata > div { grid-template-columns: 1fr; gap: .1rem; }
  }
</style>
