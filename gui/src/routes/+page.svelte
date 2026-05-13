<script>
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { loadSettings, saveSettings } from '$lib/stores/settings.js';
  import Onboarding from '$lib/components/Onboarding.svelte';
  import Sidebar from '$lib/components/Sidebar.svelte';
  import CheatPanel from '$lib/components/CheatPanel.svelte';
  import Settings from '$lib/components/Settings.svelte';

  let appSettings = $state(null);
  let loading = $state(true);
  let showSettings = $state(false);
  let adbStatus = $state(null);

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

  async function handleRerunSetup(local) {
    if (local) appSettings = local;
    showSettings = false;
  }
</script>

{#if loading}
  <div class="loading-screen">Loading…</div>
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
    --bg: #0e0e0e;
    --surface: #181818;
    --surface2: #222;
    --border: #333;
    --text: #e8e8e8;
    --text-muted: #666;
    --accent: #e8e8e8;
    --accent2: #888;
    --accent-rgb: 232,232,232;
    font-family: Inter, system-ui, -apple-system, sans-serif;
    font-size: 15px;
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

  :global(::-webkit-scrollbar) { width: 6px; }
  :global(::-webkit-scrollbar-track) { background: transparent; }
  :global(::-webkit-scrollbar-thumb) { background: var(--border); border-radius: 99px; }
  :global(select) { outline: none; }

  .loading-screen {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100vh;
    color: var(--text-muted);
    font-size: 1rem;
  }

  .app-layout {
    display: flex;
    height: 100vh;
    width: 100vw;
    overflow: hidden;
  }
</style>
