<script>
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { loadSettings, saveSettings } from '$lib/stores/settings.js';
  import Onboarding from '$lib/components/Onboarding.svelte';
  import Sidebar from '$lib/components/Sidebar.svelte';
  import CheatPanel from '$lib/components/CheatPanel.svelte';
  import Settings from '$lib/components/Settings.svelte';
  import { selectedGame } from '$lib/stores/games.js';

  let appSettings = $state(/** @type {any} */ (null));
  let loading = $state(true);
  let showSettings = $state(false);
  let adbStatus = $state(/** @type {any} */ (null));
  /** @type {'android' | 'desktop'} */
  let platform = $state('desktop');
  let isMobile = $derived((/** @type {string} */ (platform)) === 'android');
  let storagePermission = $state(/** @type {any} */ (null));
  let permissionDismissed = $state(false);

  onMount(async () => {
    try {
      platform = /** @type {any} */ (await invoke('get_platform'));
    } catch (_) {}
    try {
      appSettings = await loadSettings();
      // When running as a native Android app, force androidNative mode and
      // skip onboarding — the load path is always known.
      if (platform === 'android') {
        appSettings = {
          ...(appSettings ?? {}),
          targetMode: 'androidNative',
          onboardingDone: true,
        };
        try { await saveSettings(appSettings); } catch (_) {}
        // Check if storage permission is already granted
        try {
          storagePermission = await invoke('check_storage_permission');
        } catch (_) {}
      } else if (appSettings?.targetMode === 'android') {
        try {
          const usbDevices = await invoke('get_usb_devices', { adbPath: appSettings.adbPath });
          if (usbDevices.length > 0) {
            appSettings = {
              ...appSettings,
              activeDevice: { type: 'usb', serial: usbDevices[0], label: null },
            };
            try {
              await saveSettings(appSettings);
            } catch (e) {
              console.error('Failed to persist active device:', e);
            }
          }
        } catch (_) {}
        try {
          adbStatus = await invoke('get_adb_status', { adbPath: appSettings.adbPath });
        } catch (_) {}
      }
    } finally {
      loading = false;
    }
  });

  let saveError = $state('');

  /** @param {any} updated */
  async function handleOnboardingDone(updated) {
    try {
      await saveSettings(updated);
      appSettings = updated;
    } catch (e) {
      saveError = String(e);
      console.error('[Page] saveSettings failed:', e);
    }
    if (appSettings?.targetMode === 'android') {
      try {
        adbStatus = await invoke('get_adb_status', { adbPath: appSettings.adbPath });
      } catch (_) {}
    }
  }

  /** @param {any} local */
  async function handleSettingsClose(local) {
    if (local) {
      appSettings = local;
      if (appSettings.targetMode === 'android') {
        try {
          adbStatus = await invoke('get_adb_status', { adbPath: appSettings.adbPath });
        } catch (_) {}
      }
    }
    showSettings = false;
  }

  /** @param {any} local */
  async function handleRerunSetup(local) {
    if (local) appSettings = local;
    showSettings = false;
  }
</script>

<div class="scanlines" aria-hidden="true"></div>

{#if loading}
  <div class="loading-screen">LOADING</div>
{:else if platform === 'android' && storagePermission && !storagePermission.granted && !permissionDismissed}
  <div class="permission-screen">
    <div class="permission-box">
      <div class="permission-title">// STORAGE PERMISSION</div>
      <p class="permission-body">
        Eden Cheats Manager needs "All files access" to read and write cheat files on Android 13 and below.
      </p>
      <p class="permission-path"><code>/Android/data/dev.eden.eden_emulator/files/load/</code></p>
      <p class="permission-body">
        Tap <strong>Open Settings</strong>, then enable <strong>Allow management of all files</strong>.
      </p>
      <p class="permission-body" style="font-size:0.8em;opacity:0.7;">
        Stock Android: Special app access → All files access<br>
        Samsung One UI: Apps → Eden Cheats Manager → Permissions → Files and media → Allow management of all files
      </p>
      <button class="permission-btn" onclick={async () => {
        try { await invoke('open_storage_settings'); } catch (_) {}
      }}>
        [ OPEN SETTINGS ]
      </button>
      <button class="permission-btn" style="margin-top:0.5rem;opacity:0.7;" onclick={async () => {
        try { storagePermission = await invoke('check_storage_permission'); } catch (_) {}
      }}>
        [ CHECK AGAIN ]
      </button>
      <button class="permission-btn" style="margin-top:0.5rem;opacity:0.5;" onclick={() => permissionDismissed = true}>
        [ SKIP FOR NOW ]
      </button>
      <p class="permission-body" style="font-size:0.72em;opacity:0.5;margin-top:0.5rem;">
        You can grant this later via Settings if needed.
      </p>
    </div>
  </div>
{:else if !appSettings?.onboardingDone}
  <Onboarding currentSettings={appSettings ?? {}} ondone={handleOnboardingDone} />
  {#if saveError}<div style="position:fixed;bottom:1rem;left:1rem;right:1rem;background:#c00;color:#fff;padding:.5rem .75rem;border-radius:4px;font-size:.8rem;z-index:999">{saveError}</div>{/if}
{:else if isMobile}
  {#if !$selectedGame}
    <Sidebar settings={appSettings} {adbStatus} {platform} {isMobile} onopenSettings={() => showSettings = true} />
  {:else}
    <CheatPanel settings={appSettings} {platform} {isMobile} />
  {/if}

  {#if showSettings}
    <Settings
      settings={appSettings}
      {platform}
      onclose={handleSettingsClose}
      onrerunSetup={handleRerunSetup}
    />
  {/if}
{:else}
  <div class="app-layout">
    <Sidebar settings={appSettings} {adbStatus} {platform} onopenSettings={() => showSettings = true} />
    <CheatPanel settings={appSettings} {platform} />
  </div>

  {#if showSettings}
    <Settings
      settings={appSettings}
      {platform}
      onclose={handleSettingsClose}
      onrerunSetup={handleRerunSetup}
    />
  {/if}
{/if}


<style>
  :global(*) { box-sizing: border-box; margin: 0; padding: 0; }

  :global(:root) {
    --bg: #080600;
    --surface: #111006;
    --surface2: #1a1808;
    --border: #352510;
    --text: #f2e4a8;
    --text-muted: #9a7c4a;
    --text-bright: #fde68a;
    --text-dim: #3a2c10;
    --accent: #f5a800;
    --accent-dim: rgba(245, 168, 0, 0.08);
    --accent-glow: rgba(245, 168, 0, 0.2);
    --accent-rgb: 245, 168, 0;
    --error: #ef4444;
    --success: #c8860c;
    font-family: 'Share Tech Mono', ui-monospace, 'Cascadia Code', 'Fira Code', 'JetBrains Mono', monospace;
    font-size: 14px;
    line-height: 1.5;
    color-scheme: dark;
  }

  :global(body) {
    background: var(--bg);
    color: var(--text);
    overflow: hidden;
    height: 100vh;
    width: 100vw;
  }

  :global(::-webkit-scrollbar) { width: 5px; }
  :global(::-webkit-scrollbar-track) { background: var(--surface); }
  :global(::-webkit-scrollbar-thumb) { background: var(--border); border-radius: 0; }
  :global(::-webkit-scrollbar-thumb:hover) { background: var(--text-muted); }
  :global(select) { outline: none; }

  .scanlines {
    position: fixed;
    inset: 0;
    background: repeating-linear-gradient(
      0deg,
      transparent,
      transparent 3px,
      rgba(0, 0, 0, 0.05) 3px,
      rgba(0, 0, 0, 0.05) 4px
    );
    pointer-events: none;
    z-index: 9998;
  }

  .loading-screen {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100vh;
    color: var(--text-muted);
    font-size: 1rem;
    letter-spacing: 0.2em;
  }

  .loading-screen::after {
    content: '_';
    color: var(--accent);
    margin-left: 2px;
    animation: blink 1s step-end infinite;
  }

  @keyframes blink {
    0%, 100% { opacity: 1; }
    50%       { opacity: 0; }
  }

  .app-layout {
    display: flex;
    height: 100vh;
    width: 100vw;
    overflow: hidden;
  }

  .permission-screen {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100vh;
    padding: 1.5rem;
  }

  .permission-box {
    border: 1px solid var(--border);
    background: var(--surface);
    padding: 2rem;
    max-width: 480px;
    width: 100%;
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }

  .permission-title {
    color: var(--accent);
    font-size: 1rem;
    letter-spacing: 0.1em;
    margin-bottom: 0.5rem;
  }

  .permission-body {
    color: var(--text);
    font-size: 0.85rem;
    line-height: 1.6;
  }

  .permission-path code {
    color: var(--text-muted);
    font-size: 0.78rem;
    word-break: break-all;
  }

  .permission-btn {
    margin-top: 0.5rem;
    background: transparent;
    border: 1px solid var(--accent);
    color: var(--accent);
    padding: 0.6rem 1rem;
    font-family: inherit;
    font-size: 0.85rem;
    cursor: pointer;
    letter-spacing: 0.1em;
  }

  .permission-btn:hover {
    background: var(--accent-dim);
  }
</style>
