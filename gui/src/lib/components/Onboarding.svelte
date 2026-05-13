<script>
  import { invoke } from '@tauri-apps/api/core';
  import { onMount, untrack } from 'svelte';

  /** @type {{ currentSettings: any, ondone: function }} */
  let { currentSettings, ondone } = $props();

  // Snapshot initial prop values — untrack prevents Svelte from warning about reactive prop reads
  const _cs = /** @type {any} */ (untrack(() => currentSettings ?? {}));

  let step = $state(1);
  let mode = $state(_cs.targetMode ?? 'pc');

  let pairIp = $state((() => {
    const ad = _cs.activeDevice;
    if (ad?.type === 'wireless') return ad.serial.split(':')[0] || '';
    return '';
  })());
  let pairPort = $state('');
  let pairCode = $state('');
  let connectIp = $state('');
  let connectPort = $state('');
  let adbStatus = $state(/** @type {any} */ (null));
  let adbMsg = $state('');
  let adbBusy = $state(false);
  let adbPath = $state(_cs.adbPath ?? '');
  let pcLoadDir = $state(_cs.pcLoadDir ?? '');
  let apiToken = $state(_cs.apiToken ?? '');
  let detectedLoadDir = $state('');

  // Saved connections management
  let savedConnections = $state([...(_cs.savedConnections ?? [])]);
  let saveConnLabel = $state('');
  let showSaveConn = $state(false);
  // The IP/port of the last successful connection
  let lastConnIp = $state('');
  let lastConnPort = $state('');

  onMount(async () => {
    if (currentSettings?.onboardingDone) {
      ondone?.(buildResult());
    }
    try {
      detectedLoadDir = await invoke('detect_pc_load_dir') ?? '';
      if (!pcLoadDir) pcLoadDir = detectedLoadDir;
    } catch (_) {}
  });

  function buildResult() {
    return {
      ...currentSettings,
      targetMode: mode,
      adbPath,
      pcLoadDir,
      apiToken,
      onboardingDone: true,
      savedConnections,
      activeDevice: lastConnIp && lastConnPort
        ? { type: 'wireless', serial: `${lastConnIp}:${lastConnPort}`, label: null }
        : currentSettings?.activeDevice ?? null,
    };
  }

  async function checkAdbStatus() {
    try {
      adbStatus = await invoke('get_adb_status', { adbPath });
    } catch (e) {
      adbMsg = String(e);
    }
  }

  async function doPair() {
    adbBusy = true;
    adbMsg = '';
    try {
      await invoke('adb_pair', { adbPath, ipPort: `${pairIp}:${pairPort}`, code: pairCode });
      const ip = connectIp || pairIp;
      const port = connectPort || '5555';
      await invoke('adb_connect', { adbPath, ipPort: `${ip}:${port}` });
      await checkAdbStatus();
      if (adbStatus?.connected) {
        lastConnIp = ip;
        lastConnPort = port;
        showSaveConn = true;
        adbMsg = 'Connected.';
      } else {
        adbMsg = 'Paired but not connected. Try the connect section below.';
      }
    } catch (e) {
      adbMsg = String(e);
    } finally {
      adbBusy = false;
    }
  }

  async function doConnect() {
    adbBusy = true;
    adbMsg = '';
    try {
      await invoke('adb_connect', { adbPath, ipPort: `${connectIp}:${connectPort}` });
      await checkAdbStatus();
      if (adbStatus?.connected) {
        lastConnIp = connectIp;
        lastConnPort = connectPort;
        showSaveConn = true;
        adbMsg = 'Connected.';
      } else {
        adbMsg = 'Could not connect. Check the IP and port.';
      }
    } catch (e) {
      adbMsg = String(e);
    } finally {
      adbBusy = false;
    }
  }

  function saveConnection() {
    const label = saveConnLabel.trim() || `${lastConnIp}:${lastConnPort}`;
    // Avoid duplicates (same ip:port)
    if (!savedConnections.some(c => c.ip === lastConnIp && c.port === lastConnPort)) {
      savedConnections = [...savedConnections, { label, ip: lastConnIp, port: lastConnPort }];
    }
    showSaveConn = false;
    saveConnLabel = '';
  }

  let debugMsg = $state('');

  function advance() {
    console.log('[Onboarding] advance(), step=', step, 'mode=', mode);
    if (step === 1) {
      if (mode === 'android') { step = 2; checkAdbStatus(); }
      else step = 3;
    } else if (step === 2) {
      step = 3;
    } else {
      const result = buildResult();
      console.log('[Onboarding] calling ondone with', result);
      debugMsg = 'Finishing setup…';
      ondone?.(result);
    }
  }
</script>

<div class="onboarding">
  <div class="wizard-card">
    <div class="card-header">
      <div class="card-title-row">
        <span class="card-brand">ECM</span>
        <span class="card-title">EDEN CHEATS MANAGER</span>
      </div>
      <p class="step-label">// STEP {step} OF 3</p>
    </div>

    {#if step === 1}
      <div class="step">
        <h2>WHERE IS EDEN RUNNING?</h2>
        <div class="mode-options">
          <button class="mode-option" class:active={mode === 'pc'} onclick={() => mode = 'pc'}>
            <span class="mode-check">{mode === 'pc' ? '[*]' : '[ ]'}</span>
            <span class="mode-body">
              <strong>PC / DESKTOP</strong>
              <span>Linux, Windows, macOS</span>
            </span>
          </button>
          <button class="mode-option" class:active={mode === 'android'} onclick={() => mode = 'android'}>
            <span class="mode-check">{mode === 'android' ? '[*]' : '[ ]'}</span>
            <span class="mode-body">
              <strong>ANDROID</strong>
              <span>Via ADB (USB or Wi-Fi)</span>
            </span>
          </button>
        </div>
        <button class="btn-primary" onclick={advance}>[ CONTINUE ]</button>
      </div>

    {:else if step === 2}
      <div class="step">
        <h2>CONNECT ANDROID DEVICE</h2>
        <p class="hint">Enable <strong>Wireless debugging</strong> in Developer Options, then tap "Pair device with pairing code".</p>

        <label>
          ADB binary <span class="optional">(leave blank for system adb)</span>
          <input bind:value={adbPath} placeholder="/usr/bin/adb" />
        </label>

        <div class="status-line" class:connected={adbStatus?.connected}>
          {#if adbStatus?.connected}
            Connected: {adbStatus.deviceId}
          {:else}
            {adbStatus ? 'No device found' : 'Not checked yet'}
          {/if}
        </div>

        <div class="section">
          <h3>Pair (Android 11+)</h3>
          <div class="row">
            <label>IP <input bind:value={pairIp} placeholder="192.168.1.x" /></label>
            <label>Pairing port <input bind:value={pairPort} placeholder="37001" /></label>
          </div>
          <label>Pairing code <input bind:value={pairCode} placeholder="123456" /></label>
          <button class="btn-secondary" disabled={adbBusy || !pairIp || !pairPort || !pairCode} onclick={doPair}>
            {adbBusy ? 'Working…' : 'Pair & Connect'}
          </button>
        </div>

        <div class="section">
          <h3>Connect directly</h3>
          <div class="row">
            <label>IP <input bind:value={connectIp} placeholder="192.168.1.x" /></label>
            <label>Port <input bind:value={connectPort} placeholder="Port" /></label>
          </div>
          <button class="btn-secondary" disabled={adbBusy || !connectIp} onclick={doConnect}>
            {adbBusy ? 'Working…' : 'Connect'}
          </button>
        </div>

        {#if adbMsg}
          <p class="msg" class:ok={adbMsg.startsWith('Connected')}>{adbMsg}</p>
        {/if}

        {#if showSaveConn && adbStatus?.connected}
          <div class="save-conn-box">
            <p class="save-conn-title">Save this connection?</p>
            <div class="row">
              <label style="flex:1">
                Label
                <input bind:value={saveConnLabel} placeholder="{lastConnIp}:{lastConnPort}" />
              </label>
              <button class="btn-secondary" onclick={saveConnection}>Save</button>
              <button class="btn-link" onclick={() => showSaveConn = false}>Skip</button>
            </div>
          </div>
        {/if}

        {#if savedConnections.length > 0}
          <div class="saved-list">
            {#each savedConnections as c}
              <span class="saved-tag">{c.label}</span>
            {/each}
          </div>
        {/if}

        <div class="row-actions">
          <button class="btn-primary" onclick={advance}>[ NEXT ]</button>
          <button class="btn-link" onclick={advance}>SKIP</button>
        </div>
      </div>

    {:else if step === 3}
      <div class="step">
        <h2>API TOKEN</h2>
        <p class="hint">
          Create a free account at <a href="https://www.cheatslips.com" target="_blank" rel="noreferrer">cheatslips.com</a> to get a token. Required to download cheat files.
        </p>

        {#if mode === 'pc'}
          <div class="section">
            <h3>Eden load directory</h3>
            {#if detectedLoadDir}
              <p class="hint small">Detected: <code>{detectedLoadDir}</code></p>
            {/if}
            <label>
              Path
              <input bind:value={pcLoadDir} placeholder="~/.local/share/eden/load/" />
            </label>
          </div>
        {/if}

        <label>
          Token <span class="optional">(can add later in Settings)</span>
          <input type="password" bind:value={apiToken} placeholder="Your cheatslips.com token" />
        </label>

        <button class="btn-primary" onclick={advance}>[ FINISH SETUP ]</button>
        {#if debugMsg}<p class="msg">{debugMsg}</p>{/if}
      </div>
    {/if}
  </div>
</div>

<style>
  .onboarding {
    min-height: 100vh;
    display: flex;
    align-items: flex-start;
    justify-content: center;
    background: var(--bg);
    padding: 1.5rem 1rem;
    overflow-y: auto;
  }

  .wizard-card {
    background: var(--surface);
    border: 1px solid var(--border);
    border-top: 2px solid var(--accent);
    padding: 1.5rem;
    max-width: 460px;
    width: 100%;
    margin: auto 0;
    max-height: calc(100vh - 3rem);
    overflow-y: auto;
    box-shadow: 0 0 30px rgba(0, 0, 0, 0.6), 0 0 15px rgba(245, 168, 0, 0.05);
  }

  .card-header {
    margin-bottom: 1.1rem;
    padding-bottom: .75rem;
    border-bottom: 1px solid var(--border);
  }
  .card-title-row {
    display: flex;
    align-items: baseline;
    gap: .65rem;
    margin-bottom: .2rem;
  }
  .card-brand {
    font-size: .9rem;
    letter-spacing: .25em;
    color: var(--accent);
  }
  .card-title {
    font-size: .78rem;
    letter-spacing: .12em;
    color: var(--text-muted);
  }
  .step-label { font-size: .68rem; color: var(--text-dim); margin: 0; letter-spacing: .08em; }

  h2 {
    font-size: .85rem;
    color: var(--text);
    margin: 0 0 .6rem;
    letter-spacing: .1em;
  }
  h3 {
    font-size: .62rem;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: .1em;
    margin: 0 0 .45rem;
  }

  .step { display: flex; flex-direction: column; gap: .6rem; }

  .mode-options {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: .4rem;
  }
  .mode-option {
    background: none;
    border: 1px solid var(--border);
    padding: .65rem .75rem;
    cursor: pointer;
    text-align: left;
    display: flex;
    align-items: flex-start;
    gap: .55rem;
    color: var(--text-muted);
    transition: border-color .12s, color .12s, background .12s;
    font-family: inherit;
  }
  .mode-option:hover { border-color: var(--text-muted); color: var(--text); background: rgba(245,168,0,0.03); }
  .mode-option.active { border-color: var(--accent); color: var(--text); background: var(--accent-dim); }
  .mode-check { font-size: .82rem; color: var(--accent); flex-shrink: 0; letter-spacing: -.02em; margin-top: .05rem; }
  .mode-body { display: flex; flex-direction: column; gap: .15rem; }
  .mode-option strong { font-size: .78rem; letter-spacing: .05em; }
  .mode-option span { font-size: .68rem; }

  label { display: flex; flex-direction: column; gap: .2rem; font-size: .72rem; color: var(--text-muted); letter-spacing: .04em; }

  input {
    background: var(--surface2);
    border: 1px solid var(--border);
    color: var(--text);
    padding: .38rem .55rem;
    font-size: .78rem;
    outline: none;
    width: 100%;
    font-family: inherit;
    letter-spacing: .03em;
    transition: border-color .12s;
  }
  input:focus { border-color: var(--accent); box-shadow: 0 0 0 1px var(--accent-glow); }

  .row { display: grid; grid-template-columns: 1fr 1fr; gap: .45rem; }

  .btn-primary {
    background: var(--accent-dim);
    color: var(--accent);
    border: 1px solid var(--accent);
    padding: .45rem 1rem;
    font-size: .75rem;
    cursor: pointer;
    align-self: flex-start;
    font-family: inherit;
    letter-spacing: .12em;
    transition: background .15s, box-shadow .15s;
  }
  .btn-primary:not(:disabled):hover { background: rgba(245,168,0,0.15); box-shadow: 0 0 8px var(--accent-glow); }
  .btn-primary:disabled { opacity: .35; cursor: default; }

  .btn-secondary {
    background: none;
    color: var(--text-muted);
    border: 1px solid var(--border);
    padding: .38rem .8rem;
    font-size: .75rem;
    cursor: pointer;
    align-self: flex-start;
    font-family: inherit;
    letter-spacing: .05em;
    transition: color .12s, border-color .12s;
  }
  .btn-secondary:not(:disabled):hover { border-color: var(--text-muted); color: var(--text); }
  .btn-secondary:disabled { opacity: .35; cursor: default; }

  .btn-link {
    background: none;
    border: none;
    color: var(--text-muted);
    cursor: pointer;
    font-size: .72rem;
    padding: 0;
    letter-spacing: .06em;
    text-decoration: none;
    align-self: center;
    font-family: inherit;
    transition: color .12s;
  }
  .btn-link:hover { color: var(--text); }

  .row-actions { display: flex; align-items: center; gap: .65rem; }

  .section {
    border: 1px solid var(--border);
    padding: .65rem .75rem;
    display: flex;
    flex-direction: column;
    gap: .45rem;
  }

  .status-line {
    font-size: .72rem;
    color: var(--text-muted);
    padding: .25rem 0;
    border-bottom: 1px solid var(--border);
    letter-spacing: .04em;
  }
  .status-line.connected { color: var(--accent); }

  .msg {
    font-size: .72rem;
    color: var(--text-muted);
    margin: 0;
    padding: .3rem .5rem;
    background: var(--surface2);
    border-left: 2px solid var(--border);
    letter-spacing: .03em;
  }
  .msg.ok { color: var(--text); border-left-color: var(--accent); }

  .hint { font-size: .72rem; color: var(--text-muted); margin: 0; line-height: 1.5; }
  .hint a { color: var(--text-muted); text-decoration: underline; }
  .hint a:hover { color: var(--accent); }
  .hint.small { font-size: .68rem; }
  .optional { font-size: .65rem; color: var(--text-dim); }

  code {
    font-family: inherit;
    font-size: .75rem;
    background: var(--surface2);
    padding: .05rem .3rem;
    border: 1px solid var(--border);
  }

  .save-conn-box {
    background: var(--surface2);
    border: 1px solid var(--border);
    padding: .55rem .7rem;
    display: flex;
    flex-direction: column;
    gap: .35rem;
  }
  .save-conn-title { margin: 0; font-size: .72rem; color: var(--text); letter-spacing: .06em; }
  .save-conn-box .row { grid-template-columns: 1fr auto auto; align-items: end; }

  .saved-list { display: flex; gap: .3rem; flex-wrap: wrap; }
  .saved-tag {
    background: var(--surface2);
    border: 1px solid var(--border);
    font-size: .65rem;
    padding: .1rem .45rem;
    color: var(--text-muted);
    letter-spacing: .04em;
  }
</style>
