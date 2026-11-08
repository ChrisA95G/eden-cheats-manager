<script>
  /**
   * @type {{
   *   update: import('../api/types.js').AvailableUpdate,
   *   installation: import('../api/types.js').UpdateInstallation | null,
   *   canInstall: boolean,
   *   disabled?: boolean,
   *   oninstall: (update: import('../api/types.js').AvailableUpdate) => void | Promise<void>,
   * }}
   */
  let { update, installation, canInstall, disabled = false, oninstall } = $props();
  let current = $derived(installation?.update.version === update.version ? installation : null);
  let progress = $derived(current?.busy ? current.progress : null);
  let percent = $derived(progress?.event === 'downloading' && progress.total > 0
    ? Math.min(100, Math.floor(progress.downloaded / progress.total * 100)) : undefined);
  let status = $derived(progress?.event === 'downloading'
    ? `Downloading update${percent === undefined ? '…' : `… ${percent}%`}`
    : progress ? {
      checking: 'Checking update…',
      verifying: 'Verifying download…',
      installing: 'Installing update…',
      restarting: 'Restarting EDC…',
    }[progress.event] : '');
</script>

<div class="app-update">
  <div class="update-row">
    <div class="update-copy">
      <p aria-live="polite">{current?.busy ? `Updating to EDC ${update.version}` : `EDC ${update.version} is available.`}</p>
      {#if canInstall && !current?.busy}
        <p class="md-supporting-text">EDC will close and restart during the update.</p>
      {/if}
    </div>
    {#if canInstall}
      <button class="md-button md-button--filled" type="button"
        disabled={disabled || installation?.busy} onclick={() => oninstall(update)}>
        {current?.busy ? 'Updating…' : current?.error ? 'Retry update' : 'Update now'}
      </button>
    {/if}
  </div>
  {#if progress}
    <p role="status" aria-live="polite">{status}</p>
    <progress max="100" value={percent} aria-label="App update progress"></progress>
  {/if}
  {#if current?.error}
    <p class="md-error-text" role="alert">{current.error}</p>
  {/if}
</div>

<style>
  .app-update { min-width:0; display:grid; gap:0.5rem; }
  .update-row { display:flex; align-items:center; flex-wrap:wrap; gap:0.75rem; }
  .update-copy { flex:1; min-width:min(100%,14rem); }
  p { overflow-wrap:anywhere; }
  progress { width:100%; height:0.5rem; accent-color:var(--md-sys-color-primary); }
</style>
