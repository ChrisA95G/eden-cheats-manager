<script>
  import { invoke } from '@tauri-apps/api/core';
  import { onMount } from 'svelte';

  /** @type {{ currentSettings: any, ondone: function }} */
  let { currentSettings, ondone } = $props();

  let step = $state(1);
  let mode = $state(currentSettings?.targetMode ?? 'pc');

  let pairIp = $state((() => {
    const ad = currentSettings?.activeDevice;
    if (ad?.type === 'wireless') return ad.serial.split(':')[0] || '';
    return '';
  })());
  let pairPort = $state('');
  let pairCode = $state('');
  let connectIp = $state('');
  let connectPort = $state('');
  let adbStatus = $state(null);
  let adbMsg = $state('');
  let adbBusy = $state(false);
  let adbPath = $state(currentSettings?.adbPath ?? '');
  let pcLoadDir = $state(currentSettings?.pcLoadDir ?? '');
  let apiToken = $state(currentSettings?.apiToken ?? '');
  let detectedLoadDir = $state('');

  // Saved connections management
  let savedConnections = $state([...(currentSettings?.savedConnections ?? [])]);
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
      <h1>Eden Cheats Manager</h1>
      <p class="step-label">Step {step} of 3</p>
    </div>

    {#if step === 1}
      <div class="step">
        <h2>Where is Eden running?</h2>
        <div class="mode-options">
          <button class="mode-option" class:active={mode === 'pc'} onclick={() => mode = 'pc'}>
            <strong>PC / Desktop</strong>
            <span>Linux, Windows, macOS</span>
          </button>
          <button class="mode-option" class:active={mode === 'android'} onclick={() => mode = 'android'}>
            <strong>Android</strong>
            <span>Via ADB (USB or Wi-Fi)</span>
          </button>
        </div>
        <button class="btn-primary" onclick={advance}>Continue</button>
      </div>

    {:else if step === 2}
      <div class="step">
        <h2>Connect your Android device</h2>
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
          <button class="btn-primary" onclick={advance}>Next</button>
          <button class="btn-link" onclick={advance}>Skip</button>
        </div>
      </div>

    {:else if step === 3}
      <div class="step">
        <h2>API Token</h2>
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

        <button class="btn-primary" onclick={advance}>Finish Setup</button>
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
    border-radius: 4px;
    padding: 1.75rem;
    max-width: 460px;
    width: 100%;
    margin: auto 0;
    max-height: calc(100vh - 3rem);
    overflow-y: auto;
  }

  .card-header {
    margin-bottom: 1.25rem;
    padding-bottom: .9rem;
    border-bottom: 1px solid var(--border);
  }

  h1 { font-size: 1.1rem; font-weight: 600; color: var(--text); margin: 0 0 .2rem; }
  .step-label { font-size: .75rem; color: var(--text-muted); margin: 0; }
  h2 { font-size: .95rem; font-weight: 600; color: var(--text); margin: 0 0 .6rem; }
  h3 { font-size: .7rem; font-weight: 600; color: var(--text-muted); text-transform: uppercase; letter-spacing: .06em; margin: 0 0 .5rem; }

  .step { display: flex; flex-direction: column; gap: .65rem; }

  .mode-options {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: .5rem;
  }

  .mode-option {
    background: none;
    border: 1px solid var(--border);
    border-radius: 3px;
    padding: .7rem .75rem;
    cursor: pointer;
    text-align: left;
    display: flex;
    flex-direction: column;
    gap: .2rem;
    color: var(--text-muted);
    transition: border-color .1s, color .1s;
  }
  .mode-option:hover { border-color: var(--text-muted); color: var(--text); }
  .mode-option.active { border-color: var(--text); color: var(--text); }
  .mode-option strong { font-size: .85rem; }
  .mode-option span { font-size: .73rem; }

  label { display: flex; flex-direction: column; gap: .25rem; font-size: .78rem; color: var(--text-muted); }

  input {
    background: var(--surface2);
    border: 1px solid var(--border);
    border-radius: 3px;
    color: var(--text);
    padding: .4rem .55rem;
    font-size: .83rem;
    outline: none;
    width: 100%;
    transition: border-color .1s;
  }
  input:focus { border-color: var(--text-muted); }

  .row { display: grid; grid-template-columns: 1fr 1fr; gap: .5rem; }

  .btn-primary {
    background: var(--text);
    color: var(--bg);
    border: none;
    border-radius: 3px;
    padding: .5rem 1.1rem;
    font-size: .83rem;
    font-weight: 600;
    cursor: pointer;
    align-self: flex-start;
  }
  .btn-primary:hover { opacity: .85; }
  .btn-primary:disabled { opacity: .4; cursor: default; }

  .btn-secondary {
    background: none;
    color: var(--text);
    border: 1px solid var(--border);
    border-radius: 3px;
    padding: .45rem .9rem;
    font-size: .83rem;
    cursor: pointer;
    align-self: flex-start;
  }
  .btn-secondary:hover { border-color: var(--text-muted); }
  .btn-secondary:disabled { opacity: .4; cursor: default; }

  .btn-link {
    background: none;
    border: none;
    color: var(--text-muted);
    cursor: pointer;
    font-size: .78rem;
    padding: 0;
    text-decoration: underline;
    align-self: center;
  }

  .row-actions { display: flex; align-items: center; gap: .75rem; }

  .section {
    border: 1px solid var(--border);
    border-radius: 3px;
    padding: .75rem;
    display: flex;
    flex-direction: column;
    gap: .5rem;
  }

  .status-line {
    font-size: .78rem;
    color: var(--text-muted);
    padding: .3rem 0;
    border-bottom: 1px solid var(--border);
  }
  .status-line.connected { color: var(--text); }

  .msg {
    font-size: .78rem;
    color: var(--text-muted);
    margin: 0;
    padding: .35rem .5rem;
    background: var(--surface2);
    border-radius: 3px;
    border-left: 2px solid var(--border);
  }
  .msg.ok { color: var(--text); border-left-color: var(--text-muted); }

  .hint { font-size: .78rem; color: var(--text-muted); margin: 0; }
  .hint a { color: var(--text-muted); text-decoration: underline; }
  .hint.small { font-size: .73rem; }
  .optional { font-size: .72rem; color: var(--text-muted); font-weight: 400; }

  code {
    font-family: monospace;
    font-size: .75rem;
    background: var(--surface2);
    padding: .1rem .3rem;
    border-radius: 2px;
  }

  /* Save connection box */
  .save-conn-box {
    background: var(--surface2);
    border: 1px solid var(--border);
    border-radius: 3px;
    padding: .6rem .75rem;
    display: flex;
    flex-direction: column;
    gap: .4rem;
  }
  .save-conn-title { margin: 0; font-size: .78rem; font-weight: 600; color: var(--text); }
  .save-conn-box .row { grid-template-columns: 1fr auto auto; align-items: end; }

  .saved-list { display: flex; gap: .35rem; flex-wrap: wrap; }
  .saved-tag {
    background: var(--surface2);
    border: 1px solid var(--border);
    border-radius: 3px;
    font-size: .72rem;
    padding: .15rem .5rem;
    color: var(--text-muted);
  }
</style>
