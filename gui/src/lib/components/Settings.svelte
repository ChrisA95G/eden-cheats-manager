<script>
  import { invoke } from '@tauri-apps/api/core';
  import { open as openDialog } from '@tauri-apps/plugin-dialog';
  import { revealItemInDir } from '@tauri-apps/plugin-opener';
  import { onMount, untrack } from 'svelte';
  import { saveSettings } from '../stores/settings.js';

  /** @type {{ settings: any, platform?: string, onclose: function, onrerunSetup: function }} */
  let { settings, platform = 'desktop', onclose, onrerunSetup } = $props();

  // Snapshot prop — untrack prevents Svelte from warning about reactive prop reads at init
  const _s = /** @type {any} */ (untrack(() => settings ?? {}));
  let local = $state({ savedConnections: [], ..._s });
  let saving = $state(false);
  let saved = $state(false);
  let detectedDir = $state('');
  let detectedEdenExe = $state('');
  let adbStatus = $state(/** @type {any} */ (null));
  let checkingAdb = $state(false);
  let appLogPath = $state('');
  let edenLogPath = $state('');

  // New connection form
  let newConnLabel = $state('');
  let newConnIp = $state('');
  let newConnPort = $state('');

  // Inline edit state: { idx, field: 'ip'|'port'|'label', value }
  let editing = $state(/** @type {{ idx: number, field: string, value: string } | null} */ (null));

  onMount(async () => {
    try { detectedDir = await invoke('detect_pc_load_dir') ?? ''; } catch (_) {}
    try { detectedEdenExe = await invoke('detect_eden_exe') ?? ''; } catch (_) {}
    try { appLogPath = await invoke('get_app_log_path') ?? ''; } catch (_) {}
    if (local.targetMode === 'android') checkAdb();
    if (local.targetMode === 'pc' && local.pcLoadDir) refreshEdenLogPath(local.pcLoadDir);
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

  async function checkAdb() {
    checkingAdb = true;
    try {
      adbStatus = await invoke('get_adb_status', { adbPath: local.adbPath });
    } catch (_) {
      adbStatus = null;
    } finally {
      checkingAdb = false;
    }
  }

  function addConnection() {
    if (!newConnIp.trim()) return;
    if (!newConnPort.trim()) return;
    const label = newConnLabel.trim() || `${newConnIp.trim()}:${newConnPort.trim()}`;
    local.savedConnections = [
      ...local.savedConnections,
      { label, ip: newConnIp.trim(), port: newConnPort.trim() },
    ];
    newConnLabel = '';
    newConnIp = '';
    newConnPort = '';
  }

  /** @param {number} i */
  function removeConnection(i) {
    local.savedConnections = local.savedConnections.filter((/** @type {any} */ _, /** @type {number} */ idx) => idx !== i);
    if (editing?.idx === i) editing = null;
  }

  /**
   * @param {number} i
   * @param {string} field
   * @param {string} current
   */
  function startEdit(i, field, current) {
    editing = { idx: i, field, value: current };
  }

  function saveEdit() {
    if (!editing) return;
    const { idx, field, value } = editing;
    local.savedConnections = local.savedConnections.map((/** @type {any} */ c, /** @type {number} */ i) => {
      if (i !== idx) return c;
      return { ...c, [field]: value };
    });
    editing = null;
  }

  function cancelEdit() {
    editing = null;
  }

  /** @param {KeyboardEvent} e */
  function handleEditKeydown(e) {
    if (e.key === 'Enter') saveEdit();
    if (e.key === 'Escape') cancelEdit();
  }

  /** @param {any} conn */
  async function connectTo(conn) {
    try {
      await invoke('adb_connect', { adbPath: local.adbPath, ipPort: `${conn.ip}:${conn.port}` });
      await checkAdb();
    } catch (e) {
      console.error('connect failed:', e);
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
      <!-- Mode — hidden on native Android (mode is fixed) -->
      {#if platform !== 'android'}
        <fieldset>
          <legend>Target Mode</legend>
          <div class="mode-btns">
            <button class="mode-btn" class:active={local.targetMode === 'pc'} onclick={() => local.targetMode = 'pc'}>
              <span class="mode-check">{local.targetMode === 'pc' ? '[*]' : '[ ]'}</span>
              PC / DESKTOP
            </button>
            <button class="mode-btn" class:active={local.targetMode === 'android'} onclick={() => local.targetMode = 'android'}>
              <span class="mode-check">{local.targetMode === 'android' ? '[*]' : '[ ]'}</span>
              ANDROID (ADB)
            </button>
          </div>
        </fieldset>
      {:else}
        <fieldset>
          <legend>Target Mode</legend>
          <p class="hint">Running natively on Android — direct filesystem access, no ADB required.</p>
        </fieldset>
      {/if}

      <!-- PC Load Dir -->
      {#if local.targetMode === 'pc'}
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

      <!-- ADB -->
      {#if local.targetMode === 'android'}
        <fieldset>
          <legend>ADB Configuration</legend>
          <label>
            ADB binary path <span class="optional">(blank = system adb)</span>
            <input bind:value={local.adbPath} placeholder="/usr/bin/adb" />
          </label>
          <button class="btn-secondary sm" onclick={checkAdb} disabled={checkingAdb}>
            {checkingAdb ? '[ CHECKING... ]' : '[ CHECK CONNECTION ]'}
          </button>
          {#if adbStatus !== null}
            <div class="status-badge" class:ok={adbStatus.connected} class:warn={!adbStatus.connected}>
              {adbStatus.connected ? `Connected: ${adbStatus.deviceId}` : 'No device found'}
            </div>
          {/if}
        </fieldset>

        <!-- Saved Connections -->
        <fieldset>
          <legend>Saved Connections</legend>

          {#if local.savedConnections.length > 0}
            <div class="conn-list">
              {#each local.savedConnections as conn, i}
                {@const e = editing !== null && editing.idx === i ? editing : null}
                <!-- svelte-ignore a11y_no_static_element_interactions -->
                <div class="conn-row" onkeydown={e !== null ? handleEditKeydown : undefined}>
                  {#if e?.field === 'label'}
                    <!-- svelte-ignore a11y_autofocus -->
                    <input
                      class="conn-edit-input conn-edit-label"
                      bind:value={e.value}
                      onkeydown={handleEditKeydown}
                      autofocus
                    />
                  {:else}
                    <span class="conn-label" role="button" tabindex="0"
                      onclick={() => startEdit(i, 'label', conn.label)}
                      onkeydown={(e) => (e.key === 'Enter' || e.key === ' ') && startEdit(i, 'label', conn.label)}
                    >{conn.label}</span>
                  {/if}

                  {#if e?.field === 'ip'}
                    <!-- svelte-ignore a11y_autofocus -->
                    <input
                      class="conn-edit-input conn-edit-ip"
                      bind:value={e.value}
                      onkeydown={handleEditKeydown}
                      autofocus
                    />
                    <span class="conn-sep">:</span>
                    <span class="conn-addr" role="button" tabindex="0"
                      onclick={() => startEdit(i, 'port', conn.port)}
                      onkeydown={(ev) => (ev.key === 'Enter' || ev.key === ' ') && startEdit(i, 'port', conn.port)}
                    >{conn.port}</span>
                  {:else if e?.field === 'port'}
                    <span class="conn-addr" role="button" tabindex="0"
                      onclick={() => startEdit(i, 'ip', conn.ip)}
                      onkeydown={(ev) => (ev.key === 'Enter' || ev.key === ' ') && startEdit(i, 'ip', conn.ip)}
                    >{conn.ip}</span>
                    <span class="conn-sep">:</span>
                    <!-- svelte-ignore a11y_autofocus -->
                    <input
                      class="conn-edit-input conn-edit-port"
                      bind:value={e.value}
                      onkeydown={handleEditKeydown}
                      autofocus
                    />
                  {:else}
                    <span class="conn-addr" role="button" tabindex="0" style="cursor:text"
                      onclick={() => startEdit(i, 'ip', conn.ip)}
                      onkeydown={(e) => (e.key === 'Enter' || e.key === ' ') && startEdit(i, 'ip', conn.ip)}
                    >
                      {conn.ip}<span class="conn-sep">:</span>{conn.port}
                    </span>
                  {/if}

                  <button class="btn-conn-use" onclick={e !== null ? saveEdit : () => connectTo(conn)}>
                    {e !== null ? 'Save' : 'Use'}
                  </button>
                  <button class="btn-conn-del" onclick={e !== null ? cancelEdit : () => removeConnection(i)} title={e !== null ? 'Cancel' : 'Remove'}>
                    X
                  </button>
                </div>
              {/each}
            </div>
          {:else}
            <p class="hint">No saved connections yet.</p>
          {/if}

          <div class="conn-add">
            <p class="add-label">Add connection</p>
            <div class="conn-add-fields">
              <input bind:value={newConnLabel} placeholder="Label (e.g. Living Room TV)" />
              <input bind:value={newConnIp} placeholder="IP address" style="flex:1.2" />
              <input bind:value={newConnPort} placeholder="Port" style="flex:0.6" />
            </div>
            <button class="btn-secondary sm" disabled={!newConnIp || !newConnPort} onclick={addConnection}>
              [ ADD ]
            </button>
          </div>
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
        {#if local.targetMode === 'pc'}
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
  .mode-btns { display: flex; gap: .5rem; }
  .mode-btn {
    flex: 1;
    display: flex;
    align-items: center;
    gap: .45rem;
    padding: .38rem .65rem;
    background: none;
    border: 1px solid var(--border);
    color: var(--text-muted);
    font-family: inherit;
    font-size: .78rem;
    letter-spacing: .05em;
    cursor: pointer;
    transition: border-color .12s, color .12s, background .12s;
  }
  .mode-btn:hover { border-color: var(--text-muted); color: var(--text); }
  .mode-btn.active { border-color: var(--accent); color: var(--text); background: var(--accent-dim); }
  .mode-check { font-size: .8rem; color: var(--accent); flex-shrink: 0; letter-spacing: -.02em; }

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

  /* Saved connections */
  .conn-list { display: flex; flex-direction: column; gap: .25rem; margin-bottom: .65rem; }
  .conn-row {
    display: flex; align-items: center; gap: .35rem;
    background: var(--surface2);
    border: 1px solid var(--border);
    padding: .28rem .5rem;
    font-size: .75rem;
  }
  .conn-label { color: var(--text); flex: 1; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; cursor: text; }
  .conn-addr { color: var(--text-muted); font-size: .68rem; white-space: nowrap; cursor: text; }
  .conn-sep { color: var(--text-dim); }
  .conn-edit-input {
    background: var(--bg);
    border: 1px solid var(--accent);
    color: var(--text);
    padding: .08rem .3rem;
    font-size: .72rem;
    font-family: inherit;
    outline: none;
  }
  .conn-edit-label { flex: 1; font-size: .75rem; }
  .conn-edit-ip { width: 105px; }
  .conn-edit-port { width: 50px; }
  .btn-conn-use {
    background: none;
    border: 1px solid var(--border);
    color: var(--text-muted);
    font-size: .65rem;
    padding: .08rem .35rem;
    cursor: pointer;
    font-family: inherit;
    letter-spacing: .04em;
    white-space: nowrap;
    transition: color .1s, border-color .1s;
  }
  .btn-conn-use:hover { color: var(--accent); border-color: var(--accent); }
  .btn-conn-del {
    background: none;
    border: none;
    color: var(--text-dim);
    font-size: .68rem;
    cursor: pointer;
    padding: .08rem .2rem;
    font-family: inherit;
    transition: color .1s;
  }
  .btn-conn-del:hover { color: var(--error); }
  .conn-add { margin-top: .45rem; }
  .add-label { font-size: .62rem; color: var(--text-dim); margin: 0 0 .3rem; text-transform: uppercase; letter-spacing: .1em; }
  .conn-add-fields { display: flex; gap: .3rem; margin-bottom: .35rem; }
  .conn-add-fields input { flex: 1; padding: .3rem .5rem; font-size: .75rem; }

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
