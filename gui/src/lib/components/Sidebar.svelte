<script>
  import { invoke } from '@tauri-apps/api/core';
  import { onMount } from 'svelte';
  import { games, gamesLoading, gamesError, scanGames, selectedGame } from '../stores/games.js';

  /** @type {{ settings: any, adbStatus: any, onopenSettings: function }} */
  let { settings, adbStatus, onopenSettings } = $props();

  let expandedGroups = $state(/** @type {Set<string>} */ (new Set()));
  let usbDevices = $state(/** @type {string[]} */ ([]));

  let statusColor = $derived(adbStatus?.connected ? 'ok' : settings?.targetMode === 'pc' ? 'ok' : 'warn');
  let statusLabel = $derived(settings?.targetMode === 'pc'
    ? 'PC Mode'
    : (adbStatus?.connected ? `Android: ${adbStatus.deviceId}` : 'No device'));

  let savedConnections = $derived(settings?.savedConnections ?? []);
  let showConnPicker = $state(false);
  let connecting = $state(false);

  onMount(async () => {
    if (settings?.targetMode === 'android') {
      try {
        usbDevices = await invoke('get_usb_devices', { adbPath: settings.adbPath });
      } catch (_) {}
    }
  });

  function selectGame(/** @type {import('../stores/games.js').TitleEntry} */ entry) {
    selectedGame.set(entry);
  }

  function toggleGroup(/** @type {string} */ baseTitleId) {
    const next = new Set(expandedGroups);
    if (next.has(baseTitleId)) {
      next.delete(baseTitleId);
    } else {
      next.add(baseTitleId);
    }
    expandedGroups = next;
  }

  async function switchConnection(/** @type {any} */ conn) {
    showConnPicker = false;
    connecting = true;
    try {
      if (conn.type === 'usb') {
        settings.activeDevice = { type: 'usb', serial: conn.serial, label: null };
      } else {
        await invoke('adb_connect', {
          adbPath: settings?.adbPath ?? '',
          ipPort: `${conn.ip}:${conn.port}`,
        });
        settings.activeDevice = { type: 'wireless', serial: `${conn.ip}:${conn.port}`, label: conn.label };
      }
    } catch (_) {}
    connecting = false;
  }

  let hasUsb = $derived(usbDevices.length > 0);
  let activeDeviceId = $derived(settings?.activeDevice?.serial ?? '');
</script>

<aside class="sidebar">
  <div class="sidebar-header">
    <div class="status-dot {statusColor}"></div>
    <span class="status-label">{statusLabel}</span>
    <button class="btn-icon" title="Settings" onclick={() => onopenSettings?.()}>⚙</button>
  </div>

  {#if settings?.targetMode === 'android' && (hasUsb || savedConnections.length > 0)}
    <div class="conn-bar">
      <button
        class="conn-toggle"
        onclick={() => showConnPicker = !showConnPicker}
        disabled={connecting}
      >
        {connecting ? 'Connecting…' : 'Switch device'}
        <span class="toggle-arrow">{showConnPicker ? '▴' : '▾'}</span>
      </button>

      {#if showConnPicker}
        <div class="conn-picker">
          {#if hasUsb}
            <div class="conn-section-label">USB</div>
            {#each usbDevices as serial}
              <button class="conn-item" class:conn-active={activeDeviceId === serial} onclick={() => switchConnection({ type: 'usb', serial })}>
                <span class="conn-name">{serial}</span>
              </button>
            {/each}
          {/if}

          {#if savedConnections.length > 0}
            <div class="conn-section-label">Saved</div>
            {#each savedConnections as conn}
              <button class="conn-item" class:conn-active={activeDeviceId === `${conn.ip}:${conn.port}`} onclick={() => switchConnection(conn)}>
                <span class="conn-name">{conn.label}</span>
                <span class="conn-addr">{conn.ip}:{conn.port}</span>
              </button>
            {/each}
          {/if}
        </div>
      {/if}
    </div>
  {/if}

  <button
    class="scan-btn"
    disabled={$gamesLoading}
    onclick={() => scanGames(settings)}
  >
    {$gamesLoading ? 'Scanning…' : 'Scan Games'}
  </button>

  {#if $gamesError}
    <div class="error-msg">{$gamesError}</div>
  {/if}

  <div class="game-list">
    {#if $games.length === 0 && !$gamesLoading}
      <div class="empty-state">
        <p>No games found.</p>
        <p class="hint">Click "Scan Games" to discover your Eden game library.</p>
      </div>
    {:else}
      {#each $games as group (group.baseTitleId)}
        {@const isExpanded = expandedGroups.has(group.baseTitleId)}
        {@const hasChildren = (group.baseGame != null) || group.updates.length > 0 || group.dlcs.length > 0}
        <div class="game-group">
          <!-- Group header -->
          <button
            class="group-header"
            class:active={!isExpanded && $selectedGame?.titleId === group.baseGame?.titleId}
            onclick={() => toggleGroup(group.baseTitleId)}
          >
            <span class="expand-chevron">{isExpanded ? '▼' : '▶'}</span>
            {#if group.baseImage}
              <img src={group.baseImage} alt={group.baseName} class="game-cover" loading="lazy" />
            {:else}
              <div class="game-cover-placeholder"></div>
            {/if}
            <div class="game-info">
              <span class="game-name">{group.baseName || group.baseTitleId}</span>
              <span class="game-tid">{group.baseTitleId}</span>
            </div>
            {#if group.baseInstalled}
              <span class="installed-dot" title="Installed">●</span>
            {/if}
          </button>

          <!-- Expanded children -->
          {#if isExpanded && hasChildren}
            <div class="group-children">
              {#if group.baseGame}
                {@const baseGame = group.baseGame}
                <div class="section-label">Base Game</div>
                <button
                  class="child-item"
                  class:active={$selectedGame?.titleId === baseGame.titleId}
                  onclick={() => selectGame(baseGame)}
                >
                  {#if baseGame.image}
                    <img src={baseGame.image} alt={baseGame.name} class="game-cover" loading="lazy" />
                  {:else}
                    <div class="game-cover-placeholder"></div>
                  {/if}
                  <div class="game-info">
                    <span class="game-name">{baseGame.name || baseGame.titleId}</span>
                    <span class="game-tid">{baseGame.titleId}</span>
                  </div>
                  {#if baseGame.installed}
                    <span class="installed-dot" title="Installed">●</span>
                  {/if}
                </button>
              {/if}

              {#if group.updates.length > 0}
                <div class="section-label">Updates</div>
                {#each group.updates as update (update.titleId)}
                  <button
                    class="child-item"
                    class:active={$selectedGame?.titleId === update.titleId}
                    onclick={() => selectGame(update)}
                  >
                    {#if update.image}
                      <img src={update.image} alt={update.name} class="game-cover" loading="lazy" />
                    {:else}
                      <div class="game-cover-placeholder"></div>
                    {/if}
                    <div class="game-info">
                      <span class="game-name">{update.name || update.titleId}</span>
                      <span class="game-tid">{update.titleId}</span>
                    </div>
                    {#if update.installed}
                      <span class="installed-dot" title="Installed">●</span>
                    {/if}
                  </button>
                {/each}
              {/if}

              {#if group.dlcs.length > 0}
                <div class="section-label">DLC</div>
                {#each group.dlcs as dlc (dlc.titleId)}
                  <button
                    class="child-item"
                    class:active={$selectedGame?.titleId === dlc.titleId}
                    onclick={() => selectGame(dlc)}
                  >
                    {#if dlc.image}
                      <img src={dlc.image} alt={dlc.name} class="game-cover" loading="lazy" />
                    {:else}
                      <div class="game-cover-placeholder"></div>
                    {/if}
                    <div class="game-info">
                      <span class="game-name">{dlc.name || dlc.titleId}</span>
                      <span class="game-tid">{dlc.titleId}</span>
                    </div>
                    {#if dlc.installed}
                      <span class="installed-dot" title="Installed">●</span>
                    {/if}
                  </button>
                {/each}
              {/if}
            </div>
          {/if}
        </div>
      {/each}
    {/if}
  </div>
</aside>

<style>
  .sidebar {
    width: 280px;
    min-width: 220px;
    max-width: 320px;
    background: var(--surface);
    border-right: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    height: 100vh;
    overflow: hidden;
  }
  .sidebar-header {
    display: flex;
    align-items: center;
    gap: .5rem;
    padding: .9rem 1rem;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }
  .status-dot { width: 8px; height: 8px; border-radius: 50%; flex-shrink: 0; border: 1px solid var(--text-muted); }
  .status-dot.ok  { background: var(--text); border-color: var(--text); }
  .status-dot.warn{ background: transparent; }
  .status-dot.err { background: transparent; }
  .status-label { flex: 1; font-size: .82rem; color: var(--text-muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .btn-icon { background: none; border: none; cursor: pointer; font-size: 1.1rem; padding: 0; line-height: 1; opacity: .7; }
  .btn-icon:hover { opacity: 1; }

  .conn-bar { position: relative; padding: .35rem .75rem; border-bottom: 1px solid var(--border); flex-shrink: 0; }
  .conn-toggle {
    background: none; border: 1px solid var(--border); border-radius: 3px;
    color: var(--text-muted); font-size: .75rem; padding: .25rem .6rem; cursor: pointer; width: 100%;
    text-align: left; display: flex; justify-content: space-between; align-items: center;
  }
  .conn-toggle:hover:not(:disabled) { color: var(--text); border-color: var(--text-muted); }
  .conn-toggle:disabled { opacity: .5; cursor: default; }
  .toggle-arrow { font-size: .6rem; }
  .conn-picker {
    position: absolute; left: .75rem; right: .75rem; top: calc(100% - .35rem);
    background: var(--surface2); border: 1px solid var(--border); border-radius: 4px;
    z-index: 50; overflow: hidden; max-height: 220px; overflow-y: auto;
  }
  .conn-section-label {
    font-size: .65rem; text-transform: uppercase; letter-spacing: .04em;
    color: var(--text-muted); padding: .35rem .7rem .1rem; font-weight: 600;
  }
  .conn-item {
    display: flex; flex-direction: column; gap: .1rem;
    width: 100%; background: none; border: none; border-bottom: 1px solid var(--border);
    padding: .45rem .7rem; cursor: pointer; text-align: left; color: var(--text);
  }
  .conn-item:last-child { border-bottom: none; }
  .conn-item:hover { background: var(--surface); }
  .conn-item.conn-active { border-left: 2px solid var(--text-muted); padding-left: calc(.7rem - 2px); }
  .conn-name { font-size: .82rem; font-weight: 500; }
  .conn-addr { font-size: .72rem; color: var(--text-muted); font-family: monospace; }

  .scan-btn {
    margin: .75rem;
    background: var(--text);
    color: var(--bg);
    border: none;
    border-radius: 3px;
    padding: .5rem 1rem;
    font-size: .83rem;
    font-weight: 600;
    cursor: pointer;
    transition: opacity .15s;
    flex-shrink: 0;
  }
  .scan-btn:disabled { opacity: .5; cursor: default; }
  .scan-btn:not(:disabled):hover { opacity: .85; }
  .error-msg { margin: 0 .75rem .5rem; background: var(--surface2); color: var(--text-muted); border-radius: 3px; border-left: 2px solid var(--border); padding: .4rem .7rem; font-size: .78rem; }
  .game-list { flex: 1; overflow-y: auto; padding: .25rem .5rem; }
  .empty-state { padding: 2rem 1rem; text-align: center; color: var(--text-muted); }
  .empty-state p { margin: 0 0 .25rem; font-size: .85rem; }
  .empty-state .hint { font-size: .78rem; opacity: .7; }

  /* Group */
  .game-group { margin-bottom: 0; }
  .group-header {
    width: 100%;
    display: flex;
    align-items: center;
    gap: .6rem;
    background: none;
    border: none;
    border-left: 2px solid transparent;
    border-radius: 0;
    padding: .45rem .5rem;
    cursor: pointer;
    text-align: left;
    color: var(--text);
    transition: background .1s;
  }
  .group-header:hover { background: var(--surface2); }
  .group-header.active { background: var(--surface2); border-left-color: var(--text-muted); padding-left: calc(.5rem - 2px); }
  .expand-chevron { font-size: .65rem; width: 12px; flex-shrink: 0; color: var(--text-muted); }

  /* Children container */
  .group-children {
    padding-left: 1.4rem;
    border-left: 1px solid var(--border);
    margin-left: .95rem;
  }
  .section-label {
    font-size: .68rem;
    text-transform: uppercase;
    letter-spacing: .04em;
    color: var(--text-muted);
    padding: .35rem .4rem .15rem;
    font-weight: 600;
  }
  .child-item {
    width: 100%;
    display: flex;
    align-items: center;
    gap: .6rem;
    background: none;
    border: none;
    border-left: 2px solid transparent;
    border-radius: 0;
    padding: .35rem .5rem;
    cursor: pointer;
    text-align: left;
    color: var(--text);
    transition: background .1s;
    opacity: .65;
  }
  .child-item:hover { background: var(--surface2); opacity: .85; }
  .child-item.active { background: var(--surface2); border-left-color: var(--text-muted); padding-left: calc(.5rem - 2px); opacity: 1; }

  .game-cover { width: 38px; height: 38px; border-radius: 3px; object-fit: cover; flex-shrink: 0; }
  .game-cover-placeholder { width: 38px; height: 38px; border-radius: 3px; background: var(--surface2); border: 1px solid var(--border); flex-shrink: 0; }
  .game-info { display: flex; flex-direction: column; gap: .1rem; overflow: hidden; flex: 1; }
  .game-name { font-size: .85rem; font-weight: 500; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .game-tid { font-size: .72rem; color: var(--text-muted); font-family: monospace; }
  .installed-dot { color: #34c759; font-size: .6rem; flex-shrink: 0; line-height: 1; }
</style>
