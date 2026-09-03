<script>
  import { invoke } from '@tauri-apps/api/core';
  import { open as openDialog } from '@tauri-apps/plugin-dialog';
  import { revealItemInDir } from '@tauri-apps/plugin-opener';
  import { onMount, untrack } from 'svelte';
  import { saveSettings } from '../stores/settings.js';
  import PackageDiscovery from './PackageDiscovery.svelte';

  /** @type {{ settings: any, platform?: string, onclose: function, onrerunSetup: function }} */
  let { settings, platform = 'desktop', onclose, onrerunSetup } = $props();

  // Snapshot prop — untrack prevents Svelte from warning about reactive prop reads at init
  const _s = /** @type {any} */ (untrack(() => settings ?? {}));
  let local = $state({ ..._s });
  let saving = $state(false);
  let saved = $state(false);
  let detectedDir = $state('');
  let detectedEdenExe = $state('');
  let appLogPath = $state('');
  let edenLogPath = $state('');
  let safTestResult = $state('');
  let testingSaf = $state(false);

  onMount(async () => {
    try { appLogPath = await invoke('get_app_log_path') ?? ''; } catch (_) {}
    if (platform !== 'android') {
      try { detectedDir = await invoke('detect_pc_load_dir') ?? ''; } catch (_) {}
      try { detectedEdenExe = await invoke('detect_eden_exe') ?? ''; } catch (_) {}
      if (local.pcLoadDir) refreshEdenLogPath(local.pcLoadDir);
    }
  });

  /** @param {string} loadDir */
  async function refreshEdenLogPath(loadDir) {
    try { edenLogPath = await invoke('get_eden_log_path_pc', { loadDir }) ?? ''; } catch (_) { edenLogPath = ''; }
  }

  async function openAppLog() {
    if (appLogPath) await revealItemInDir(appLogPath);
  }

  async function openEdenLog() {
    if (edenLogPath) await revealItemInDir(edenLogPath);
  }

  async function browseLoadDir() {
    const selected = await openDialog({ directory: true, title: 'Select Eden load directory' });
    if (selected) {
      local.pcLoadDir = selected;
      refreshEdenLogPath(selected);
    }
  }

  async function browseEdenExe() {
    const selected = await openDialog({ directory: false, title: 'Select Eden executable' });
    if (selected) local.edenExePath = selected;
  }

  async function selectEdenLoadDirectory() {
    safTestResult = '';
    try {
      await invoke('select_eden_load_directory');
    } catch (e) {
      safTestResult = `ERROR: ${e}`;
    }
  }

  async function testEdenLoadDirectory() {
    testingSaf = true;
    try {
      safTestResult = String(await invoke('test_eden_load_directory'));
    } catch (e) {
      safTestResult = `ERROR: ${e}`;
    } finally {
      testingSaf = false;
    }
  }

  async function save() {
    saving = true;
    try {
      await saveSettings(local);
      saved = true;
      setTimeout(() => { saved = false; onclose?.(local); }, 800);
    } finally {
      saving = false;
    }
  }
</script>

<div
  class="modal-backdrop"
  role="presentation"
  onclick={(e) => { if (e.target === e.currentTarget) onclose?.(settings); }}
  onkeydown={(e) => { if (e.key === 'Escape') onclose?.(settings); }}
>
  <div class="modal" role="dialog" aria-modal="true" aria-label="Settings">
    <div class="modal-header">
      <div class="modal-title-row">
        <span class="modal-brand">ECM</span>
        <h2>// SETTINGS</h2>
      </div>
      <button class="btn-close" onclick={() => onclose?.(settings)}>[ X ]</button>
    </div>

    <div class="modal-body">
      {#if platform === 'android'}
        <fieldset>
          <legend>Eden Load Directory — SAF Test</legend>
          <p class="hint">Select <strong>Eden → load</strong> in Android's folder picker, then test access.</p>
          <div class="saf-actions">
            <button class="btn-secondary sm" onclick={selectEdenLoadDirectory}>[ SELECT DIRECTORY ]</button>
            <button class="btn-secondary sm" onclick={testEdenLoadDirectory} disabled={testingSaf}>
              {testingSaf ? '[ TESTING... ]' : '[ TEST ACCESS ]'}
            </button>
          </div>
          {#if safTestResult}
            <div class="status-badge" class:ok={safTestResult === 'OK'} class:warn={safTestResult !== 'OK'}>
              {safTestResult === 'OK' ? 'SAF access works.' : safTestResult}
            </div>
          {/if}
        </fieldset>

        <fieldset>
          <legend>Package Build ID</legend>
          <PackageDiscovery />
        </fieldset>

      {/if}

      <!-- PC Load Dir -->
      {#if platform !== 'android'}
        <fieldset>
          <legend>Eden Load Directory</legend>
          {#if detectedDir}
            <p class="hint">Auto-detected: <code>{detectedDir}</code></p>
          {/if}
          <label>
            Path
            <div class="path-row">
              <input bind:value={local.pcLoadDir} placeholder="~/.local/share/eden/load/" />
              <button class="btn-browse" onclick={browseLoadDir} title="Browse for folder">[ … ]</button>
            </div>
          </label>
        </fieldset>

        <fieldset>
          <legend>Eden Executable</legend>
          {#if detectedEdenExe}
            <p class="hint">Auto-detected: <code>{detectedEdenExe}</code></p>
          {/if}
          <label>
            Path <span class="optional">(blank = auto-detect from PATH)</span>
            <div class="path-row">
              <input bind:value={local.edenExePath} placeholder={detectedEdenExe || '/usr/bin/eden'} />
              <button class="btn-browse" onclick={browseEdenExe} title="Browse for Eden executable">[ … ]</button>
            </div>
          </label>
          <p class="hint">Required for <strong>Scan Build ID</strong> — used to launch the game automatically.</p>
        </fieldset>
      {/if}

      <!-- API Token -->
      <fieldset>
        <legend>Cheatslips API Token</legend>
        <p class="hint">Required to download cheat file content. Get one free at cheatslips.com.</p>
        <label>
          Token
          <input type="password" bind:value={local.apiToken} placeholder="Your API token" />
        </label>
      </fieldset>

      <!-- Logs -->
      <fieldset>
        <legend>Logs</legend>
        <div class="log-row">
          <div class="log-info">
            <span class="log-label">App Log</span>
            <code class="log-path">{appLogPath || '…'}</code>
          </div>
          <button class="btn-secondary sm" disabled={!appLogPath} onclick={openAppLog}>[ OPEN ]</button>
        </div>
        {#if platform !== 'android'}
          <div class="log-row">
            <div class="log-info">
              <span class="log-label">Eden Log</span>
              <code class="log-path">{edenLogPath || '…'}</code>
            </div>
            <button class="btn-secondary sm" disabled={!edenLogPath} onclick={openEdenLog}>[ OPEN ]</button>
          </div>
          <p class="hint">Eden log is used for build ID scanning. Launch a game once to create it.</p>
        {/if}
      </fieldset>

      <!-- Re-run onboarding -->
      <div class="danger-zone">
        <button class="btn-ghost" onclick={() => { local.onboardingDone = false; onrerunSetup?.(local); }}>
          [ RE-RUN SETUP ]
        </button>
      </div>
    </div>

    <div class="modal-footer">
      <button class="btn-secondary" onclick={() => onclose?.(settings)}>[ CANCEL ]</button>
      <button class="btn-primary" disabled={saving} onclick={save}>
        {saving ? '[ SAVING... ]' : saved ? '[ SAVED ]' : '[ SAVE ]'}
      </button>
    </div>
  </div>
</div>

<style>
  .modal-backdrop {
    position: fixed; inset: 0;
    background: rgba(0,0,0,.7);
    display: flex; align-items: center; justify-content: center;
    z-index: 100;
    padding: 1rem;
  }
  .modal {
    background: var(--surface);
    border: 1px solid var(--border);
    border-top: 2px solid var(--accent);
    width: 100%;
    max-width: 500px;
    max-height: 90vh;
    display: flex;
    flex-direction: column;
    box-shadow: 0 0 40px rgba(0,0,0,.7), 0 0 20px rgba(245,168,0,.05);
  }
  .modal-header {
    display: flex; align-items: center; justify-content: space-between;
    padding: .85rem 1.25rem;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
    background: var(--surface2);
  }
  .modal-title-row {
    display: flex;
    align-items: baseline;
    gap: .6rem;
  }
  .modal-brand {
    font-size: .82rem;
    letter-spacing: .2em;
    color: var(--accent);
  }
  .modal-header h2 { margin: 0; font-size: .78rem; color: var(--text-muted); letter-spacing: .1em; }
  .btn-close {
    background: none;
    border: 1px solid var(--border);
    color: var(--text-muted);
    font-size: .65rem;
    cursor: pointer;
    padding: .15rem .35rem;
    letter-spacing: .05em;
    font-family: inherit;
    transition: color .12s, border-color .12s;
  }
  .btn-close:hover { color: var(--error); border-color: var(--error); }

  .modal-body { overflow-y: auto; padding: .85rem 1.25rem; display: flex; flex-direction: column; gap: .85rem; }
  .modal-footer {
    display: flex; justify-content: flex-end; gap: .6rem;
    padding: .75rem 1.25rem;
    border-top: 1px solid var(--border);
    flex-shrink: 0;
    background: var(--surface2);
  }

  fieldset { border: 1px solid var(--border); padding: .65rem .85rem; }
  legend { font-size: .62rem; text-transform: uppercase; letter-spacing: .1em; color: var(--text-muted); padding: 0 .35rem; }
  label { display: flex; flex-direction: column; gap: .22rem; font-size: .72rem; color: var(--text-muted); margin-bottom: .45rem; letter-spacing: .04em; }
  input {
    background: var(--surface2);
    border: 1px solid var(--border);
    color: var(--text);
    padding: .38rem .6rem;
    font-size: .8rem;
    outline: none;
    font-family: inherit;
    transition: border-color .12s;
    width: 100%;
  }
  input:focus { border-color: var(--accent); box-shadow: 0 0 0 1px var(--accent-glow); }
  .path-row { display: flex; gap: .35rem; }
  .path-row input { flex: 1; }
  .btn-browse {
    background: none;
    border: 1px solid var(--border);
    color: var(--text-muted);
    font-size: .72rem;
    padding: .38rem .55rem;
    cursor: pointer;
    font-family: inherit;
    white-space: nowrap;
    flex-shrink: 0;
    transition: color .1s, border-color .1s;
  }
  .btn-browse:hover { color: var(--accent); border-color: var(--accent); }
  .hint { font-size: .72rem; color: var(--text-muted); margin: 0 0 .45rem; line-height: 1.5; }
  .optional { font-size: .65rem; color: var(--text-dim); }
  code { font-family: inherit; font-size: .72rem; background: var(--surface2); padding: .05rem .3rem; border: 1px solid var(--border); }
  .status-badge { margin-top: .45rem; padding: .3rem .65rem; font-size: .75rem; border-left: 2px solid transparent; }
  .status-badge.ok   { background: rgba(74,222,128,.08); color: var(--success); border-left-color: var(--success); }
  .status-badge.warn { background: rgba(245,168,0,.08); color: var(--accent); border-left-color: var(--accent); }
  .saf-actions { display: flex; flex-wrap: wrap; gap: .4rem; }

  .btn-primary, .btn-secondary {
    padding: .42rem 1rem;
    font-size: .72rem;
    cursor: pointer;
    font-family: inherit;
    letter-spacing: .08em;
    transition: background .15s, box-shadow .15s, color .12s, border-color .12s;
  }
  .btn-primary {
    background: var(--accent-dim);
    color: var(--accent);
    border: 1px solid var(--accent);
  }
  .btn-primary:not(:disabled):hover { background: rgba(245,168,0,0.15); box-shadow: 0 0 8px var(--accent-glow); }
  .btn-secondary {
    background: none;
    color: var(--text-muted);
    border: 1px solid var(--border);
  }
  .btn-secondary:not(:disabled):hover { color: var(--text); border-color: var(--text-muted); }
  .btn-primary:disabled, .btn-secondary:disabled { opacity: .35; cursor: default; }
  .btn-secondary.sm { padding: .28rem .7rem; font-size: .68rem; margin-top: .2rem; }

  .btn-ghost {
    background: none;
    border: none;
    color: var(--text-muted);
    font-size: .72rem;
    cursor: pointer;
    padding: .2rem 0;
    letter-spacing: .05em;
    font-family: inherit;
    transition: color .12s;
  }
  .btn-ghost:hover { color: var(--error); }
  .danger-zone { margin-top: .35rem; }
  p { margin: 0 0 .35rem; font-size: .78rem; color: var(--text-muted); }

  .log-row {
    display: flex;
    align-items: center;
    gap: .5rem;
    margin-bottom: .35rem;
  }
  .log-info { display: flex; flex-direction: column; flex: 1; gap: .15rem; min-width: 0; }
  .log-label { font-size: .62rem; text-transform: uppercase; letter-spacing: .08em; color: var(--text-dim); }
  .log-path {
    font-size: .65rem;
    color: var(--text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    display: block;
    background: none;
    border: none;
    padding: 0;
  }
</style>
