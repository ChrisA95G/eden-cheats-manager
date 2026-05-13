<script>
  import { invoke } from '@tauri-apps/api/core';
  import { onMount } from 'svelte';
  import { saveSettings } from '../stores/settings.js';

  /** @type {{ settings: any, onclose: function, onrerunSetup: function }} */
  let { settings, onclose, onrerunSetup } = $props();

  let local = $state({
    savedConnections: [],
    ...settings,
  });
  let saving = $state(false);
  let saved = $state(false);
  let detectedDir = $state('');
  let adbStatus = $state(null);
  let checkingAdb = $state(false);

  // New connection form
  let newConnLabel = $state('');
  let newConnIp = $state('');
  let newConnPort = $state('');

  // Inline edit state: { idx, field: 'ip'|'port'|'label', value }
  let editing = $state(/** @type {{ idx: number, field: string, value: string } | null} */ (null));

  onMount(async () => {
    try {
      detectedDir = await invoke('detect_pc_load_dir') ?? '';
    } catch (_) {}
    if (local.targetMode === 'android') checkAdb();
  });

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

  function removeConnection(i) {
    local.savedConnections = local.savedConnections.filter((_, idx) => idx !== i);
    if (editing?.idx === i) editing = null;
  }

  function startEdit(i, field, current) {
    editing = { idx: i, field, value: current };
  }

  function saveEdit() {
    if (!editing) return;
    const { idx, field, value } = editing;
    local.savedConnections = local.savedConnections.map((c, i) => {
      if (i !== idx) return c;
      return { ...c, [field]: value };
    });
    editing = null;
  }

  function cancelEdit() {
    editing = null;
  }

  function handleEditKeydown(e) {
    if (e.key === 'Enter') saveEdit();
    if (e.key === 'Escape') cancelEdit();
  }

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
      <h2>Settings</h2>
      <button class="btn-close" onclick={() => onclose?.(settings)}>✕</button>
    </div>

    <div class="modal-body">
      <!-- Mode -->
      <fieldset>
        <legend>Target Mode</legend>
        <div class="radio-group">
          <label class="radio-label">
            <input type="radio" bind:group={local.targetMode} value="pc" />
            🖥️ PC / Desktop
          </label>
          <label class="radio-label">
            <input type="radio" bind:group={local.targetMode} value="android" />
            📱 Android (ADB)
          </label>
        </div>
      </fieldset>

      <!-- PC Load Dir -->
      {#if local.targetMode === 'pc'}
        <fieldset>
          <legend>Eden Load Directory</legend>
          {#if detectedDir}
            <p class="hint">Auto-detected: <code>{detectedDir}</code></p>
          {/if}
          <label>
            Path
            <input bind:value={local.pcLoadDir} placeholder="~/.local/share/eden/load/" />
          </label>
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
            {checkingAdb ? 'Checking…' : 'Check connection'}
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
                {@const isEditing = editing?.idx === i}
                <!-- svelte-ignore a11y_no_static_element_interactions -->
                <div class="conn-row" onkeydown={isEditing ? handleEditKeydown : undefined}>
                  {#if isEditing && editing.field === 'label'}
                    <input
                      class="conn-edit-input conn-edit-label"
                      bind:value={editing.value}
                      onkeydown={handleEditKeydown}
                      autofocus
                    />
                  {:else}
                    <span class="conn-label" role="button" tabindex="0"
                      onclick={() => startEdit(i, 'label', conn.label)}
                      onkeydown={(e) => (e.key === 'Enter' || e.key === ' ') && startEdit(i, 'label', conn.label)}
                    >{conn.label}</span>
                  {/if}

                  {#if isEditing && editing.field === 'ip'}
                    <input
                      class="conn-edit-input conn-edit-ip"
                      bind:value={editing.value}
                      onkeydown={handleEditKeydown}
                      autofocus
                    />
                    <span class="conn-sep">:</span>
                    <span class="conn-addr" role="button" tabindex="0"
                      onclick={() => startEdit(i, 'port', conn.port)}
                      onkeydown={(e) => (e.key === 'Enter' || e.key === ' ') && startEdit(i, 'port', conn.port)}
                    >{conn.port}</span>
                  {:else if isEditing && editing.field === 'port'}
                    <span class="conn-addr" role="button" tabindex="0"
                      onclick={() => startEdit(i, 'ip', conn.ip)}
                      onkeydown={(e) => (e.key === 'Enter' || e.key === ' ') && startEdit(i, 'ip', conn.ip)}
                    >{conn.ip}</span>
                    <span class="conn-sep">:</span>
                    <input
                      class="conn-edit-input conn-edit-port"
                      bind:value={editing.value}
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

                  <button class="btn-conn-use" onclick={isEditing ? saveEdit : () => connectTo(conn)}>
                    {isEditing ? 'Save' : 'Use'}
                  </button>
                  <button class="btn-conn-del" onclick={isEditing ? cancelEdit : () => removeConnection(i)} title={isEditing ? 'Cancel' : 'Remove'}>
                    ✕
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
              Add
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

      <!-- Re-run onboarding -->
      <div class="danger-zone">
        <button class="btn-ghost" onclick={() => { local.onboardingDone = false; onrerunSetup?.(local); }}>
          ↩ Re-run setup wizard
        </button>
      </div>
    </div>

    <div class="modal-footer">
      <button class="btn-secondary" onclick={() => onclose?.(settings)}>Cancel</button>
      <button class="btn-primary" disabled={saving} onclick={save}>
        {saving ? 'Saving…' : saved ? '✓ Saved' : 'Save'}
      </button>
    </div>
  </div>
</div>

<style>
  .modal-backdrop {
    position: fixed; inset: 0;
    background: rgba(0,0,0,.55);
    display: flex; align-items: center; justify-content: center;
    z-index: 100;
    padding: 1rem;
  }
  .modal {
    background: var(--surface);
    border-radius: 14px;
    width: 100%;
    max-width: 500px;
    max-height: 90vh;
    display: flex;
    flex-direction: column;
    box-shadow: 0 12px 48px rgba(0,0,0,.5);
  }
  .modal-header {
    display: flex; align-items: center; justify-content: space-between;
    padding: 1.1rem 1.5rem;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }
  .modal-header h2 { margin: 0; font-size: 1.1rem; }
  .btn-close { background: none; border: none; color: var(--text-muted); font-size: 1rem; cursor: pointer; padding: .2rem; }
  .btn-close:hover { color: var(--text); }
  .modal-body { overflow-y: auto; padding: 1rem 1.5rem; display: flex; flex-direction: column; gap: 1rem; }
  .modal-footer {
    display: flex; justify-content: flex-end; gap: .75rem;
    padding: 1rem 1.5rem;
    border-top: 1px solid var(--border);
    flex-shrink: 0;
  }

  fieldset { border: 1px solid var(--border); border-radius: 8px; padding: .75rem 1rem; }
  legend { font-size: .8rem; font-weight: 600; color: var(--text-muted); padding: 0 .4rem; }
  label { display: flex; flex-direction: column; gap: .3rem; font-size: .82rem; color: var(--text-muted); margin-bottom: .5rem; }
  input {
    background: var(--surface2); border: 1px solid var(--border); border-radius: 6px;
    color: var(--text); padding: .45rem .7rem; font-size: .88rem; outline: none;
  }
  input:focus { border-color: var(--accent); }
  .radio-group { display: flex; gap: 1.5rem; }
  .radio-label { flex-direction: row !important; align-items: center; gap: .4rem; cursor: pointer; font-size: .9rem !important; color: var(--text) !important; }
  .hint { font-size: .78rem; color: var(--text-muted); margin: 0 0 .5rem; }
  .optional { font-size: .72rem; color: var(--text-muted); }
  code { font-family: monospace; font-size: .78rem; background: var(--surface2); padding: .1rem .35rem; border-radius: 3px; }
  .status-badge { margin-top: .5rem; padding: .35rem .75rem; border-radius: 6px; font-size: .82rem; }
  .status-badge.ok  { background: rgba(52,199,89,.15); color: #34c759; }
  .status-badge.warn{ background: rgba(255,204,0,.15); color: #ffc500; }

  .btn-primary, .btn-secondary {
    border: none; border-radius: 8px; padding: .55rem 1.25rem;
    font-size: .9rem; font-weight: 600; cursor: pointer; transition: opacity .15s;
  }
  .btn-primary { background: var(--accent); color: #fff; }
  .btn-secondary { background: var(--surface2); color: var(--text); border: 1px solid var(--border); }
  .btn-primary:disabled, .btn-secondary:disabled { opacity: .4; cursor: default; }
  .btn-primary:not(:disabled):hover, .btn-secondary:not(:disabled):hover { opacity: .85; }
  .btn-secondary.sm { padding: .35rem .9rem; font-size: .82rem; margin-top: .25rem; }
  .btn-ghost { background: none; border: none; color: var(--text-muted); font-size: .82rem; cursor: pointer; padding: .25rem 0; text-decoration: underline; }
  .btn-ghost:hover { color: #ff3b30; }
  .danger-zone { margin-top: .5rem; }
  p { margin: 0 0 .4rem; font-size: .85rem; color: var(--text-muted); }

  /* Saved connections */
  .conn-list { display: flex; flex-direction: column; gap: .3rem; margin-bottom: .75rem; }
  .conn-row {
    display: flex; align-items: center; gap: .4rem;
    background: var(--surface2); border-radius: 4px; padding: .3rem .5rem;
    font-size: .82rem;
  }
  .conn-label { font-weight: 500; color: var(--text); flex: 1; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; cursor: text; }
  .conn-addr { color: var(--text-muted); font-family: monospace; font-size: .78rem; white-space: nowrap; cursor: text; }
  .conn-sep { color: var(--text-muted); }
  .conn-edit-input {
    background: var(--surface); border: 1px solid var(--text-muted); border-radius: 3px;
    color: var(--text); padding: .1rem .3rem; font-size: .78rem; font-family: monospace; outline: none;
  }
  .conn-edit-label { flex: 1; font-family: inherit; font-size: .82rem; font-weight: 500; }
  .conn-edit-ip { width: 110px; }
  .conn-edit-port { width: 52px; }
  .btn-conn-use {
    background: none; border: 1px solid var(--border); border-radius: 3px;
    color: var(--text-muted); font-size: .72rem; padding: .1rem .4rem; cursor: pointer;
    white-space: nowrap;
  }
  .btn-conn-use:hover { color: var(--text); border-color: var(--text-muted); }
  .btn-conn-del {
    background: none; border: none; color: var(--text-muted); font-size: .75rem;
    cursor: pointer; padding: .1rem .2rem; line-height: 1;
  }
  .btn-conn-del:hover { color: #ff3b30; }
  .conn-add { margin-top: .5rem; }
  .add-label { font-size: .72rem; color: var(--text-muted); margin: 0 0 .35rem; text-transform: uppercase; letter-spacing: .05em; font-weight: 600; }
  .conn-add-fields { display: flex; gap: .35rem; margin-bottom: .4rem; }
  .conn-add-fields input { flex: 1; padding: .35rem .55rem; font-size: .82rem; }
</style>
