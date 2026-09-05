<script>
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { loadSettings, saveSettings } from '$lib/stores/settings.js';
  import Onboarding from '$lib/components/Onboarding.svelte';
  import Sidebar from '$lib/components/Sidebar.svelte';
  import CheatPanel from '$lib/components/CheatPanel.svelte';
  import Settings from '$lib/components/Settings.svelte';
  import NativeSafSetup from '$lib/components/NativeSafSetup.svelte';
  import { selectedGame } from '$lib/stores/games.js';

  let appSettings = $state(/** @type {any} */ (null));
  let loading = $state(true);
  let showSettings = $state(false);
  /** @type {'android' | 'desktop'} */
  let platform = $state('desktop');
  let isMobile = $derived((/** @type {string} */ (platform)) === 'android');
  let nativeSafStatus = $state(/** @type {any} */ (null));

  onMount(async () => {
    try {
      platform = /** @type {any} */ (await invoke('get_platform'));
    } catch (_) {}
    try {
      appSettings = await loadSettings();
      // Native Android always uses the SAF backend, but the main UI is only
      // available after the exact Eden load grant has been verified.
      if (platform === 'android') {
        try {
          nativeSafStatus = await invoke('get_eden_load_access_status');
        } catch (e) {
          nativeSafStatus = {
            ready: false,
            message: String(e),
          };
        }
        appSettings = {
          ...(appSettings ?? {}),
          onboardingDone: nativeSafStatus?.ready === true,
        };
        try { await saveSettings(appSettings); } catch (_) {}
      }
    } finally {
      loading = false;
    }
  });

  let saveError = $state('');

  /** @param {any} status */
  async function handleNativeSafReady(status) {
    nativeSafStatus = status;
    appSettings = {
      ...(appSettings ?? {}),
      onboardingDone: true,
    };
    try {
      await saveSettings(appSettings);
    } catch (e) {
      saveError = String(e);
    }
  }

  /** @param {any} updated */
  async function handleOnboardingDone(updated) {
    try {
      await saveSettings(updated);
      appSettings = updated;
    } catch (e) {
      saveError = String(e);
      console.error('[Page] saveSettings failed:', e);
    }
  }

  /** @param {any} local */
  async function handleSettingsClose(local) {
    if (local) {
      appSettings = local;
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
{:else if platform === 'android' && !nativeSafStatus?.ready}
  <NativeSafSetup initialStatus={nativeSafStatus} onready={handleNativeSafReady} />
{:else if !appSettings?.onboardingDone}
  <Onboarding currentSettings={appSettings ?? {}} ondone={handleOnboardingDone} />
  {#if saveError}<div style="position:fixed;bottom:1rem;left:1rem;right:1rem;background:#c00;color:#fff;padding:.5rem .75rem;border-radius:4px;font-size:.8rem;z-index:999">{saveError}</div>{/if}
{:else if isMobile}
  {#if !$selectedGame}
    <Sidebar settings={appSettings} {platform} {isMobile} onopenSettings={() => showSettings = true} />
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
    <Sidebar settings={appSettings} {platform} onopenSettings={() => showSettings = true} />
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
