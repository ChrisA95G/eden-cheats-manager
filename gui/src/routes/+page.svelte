<script>
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { loadSettings, saveSettings } from '$lib/stores/settings.js';
  import Onboarding from '$lib/components/Onboarding.svelte';
  import Sidebar from '$lib/components/Sidebar.svelte';
  import CheatPanel from '$lib/components/CheatPanel.svelte';
  import Settings from '$lib/components/Settings.svelte';

  let appSettings = $state(/** @type {any} */ (null));
  let loading = $state(true);
  let showSettings = $state(false);
  let adbStatus = $state(/** @type {any} */ (null));

  onMount(async () => {
    try {
      appSettings = await loadSettings();
      if (appSettings?.targetMode === 'android') {
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
{:else if !appSettings?.onboardingDone}
  <Onboarding currentSettings={appSettings ?? {}} ondone={handleOnboardingDone} />
  {#if saveError}<div style="position:fixed;bottom:1rem;left:1rem;right:1rem;background:#c00;color:#fff;padding:.5rem .75rem;border-radius:4px;font-size:.8rem;z-index:999">{saveError}</div>{/if}
{:else}
  <div class="app-layout">
    <Sidebar settings={appSettings} {adbStatus} onopenSettings={() => showSettings = true} />
    <CheatPanel settings={appSettings} />
  </div>

  {#if showSettings}
    <Settings
      settings={appSettings}
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
</style>
