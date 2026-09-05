<script>
  import { untrack } from 'svelte';
  import Icon from './ui/Icon.svelte';

  /** @typedef {import('../api/types.js').AppSettings} AppSettings */
  /** @typedef {import('../api/types.js').EdenLoadAccessStatus} EdenLoadAccessStatus */
  /** @typedef {import('../api/types.js').Platform} Platform */
  /** @typedef {'edenLoad' | 'prodKeys' | 'packageLibrary'} DesktopPickerKind */

  /**
   * @typedef {Object} Props
   * @property {Platform} platform
   * @property {AppSettings} settings
   * @property {string} [detectedPcLoadDir]
   * @property {EdenLoadAccessStatus | null} [edenAccess]
   * @property {boolean} [busy]
   * @property {string} [error]
   * @property {(settings: AppSettings) => void | Promise<void>} [onsubmit]
   * @property {(kind: DesktopPickerKind, currentPath: string) => string | null | Promise<string | null>} [onpickdesktop]
   * @property {() => void | Promise<void>} [onpickandroid]
   * @property {() => void | Promise<void>} [onretryandroid]
   */
  /** @type {Props} */
  let {
    platform,
    settings,
    detectedPcLoadDir = '',
    edenAccess = null,
    busy = false,
    error = '',
    onsubmit,
    onpickdesktop,
    onpickandroid,
    onretryandroid,
  } = $props();

  const initialSettings = untrack(() => settings);
  const initialDetectedLoadDir = untrack(() => detectedPcLoadDir);

  let pcLoadDir = $state(initialSettings.pcLoadDir || initialDetectedLoadDir || '');
  let prodKeysPath = $state(initialSettings.prodKeysPath || '');
  let packageLibraryPath = $state(initialSettings.packageLibraryPath || '');
  let apiToken = $state(initialSettings.apiToken || '');
  let localError = $state('');
  let activeAction = $state('');
  let loadDirEdited = $state(false);

  let packagePairIncomplete = $derived(
    Boolean(prodKeysPath.trim()) !== Boolean(packageLibraryPath.trim()),
  );
  let androidReady = $derived(edenAccess?.ready === true);
  let componentBusy = $derived(busy || Boolean(activeAction));
  let displayedError = $derived(localError || error);

  $effect(() => {
    if (!loadDirEdited && !pcLoadDir && detectedPcLoadDir) {
      pcLoadDir = detectedPcLoadDir;
    }
  });

  /** @param {unknown} cause */
  function errorMessage(cause) {
    return cause instanceof Error ? cause.message : String(cause);
  }

  /**
   * @template T
   * @param {string} name
   * @param {() => T | Promise<T>} action
   * @returns {Promise<T | undefined>}
   */
  async function runAction(name, action) {
    if (componentBusy) return undefined;
    activeAction = name;
    localError = '';
    try {
      return await action();
    } catch (cause) {
      localError = errorMessage(cause);
      return undefined;
    } finally {
      activeAction = '';
    }
  }

  /** @param {DesktopPickerKind} kind @param {string} currentPath */
  async function pickDesktop(kind, currentPath) {
    const selected = await runAction(`desktop-${kind}`, async () => {
      if (!onpickdesktop) throw new Error('The desktop picker is unavailable.');
      return onpickdesktop(kind, currentPath);
    });
    if (typeof selected !== 'string' || !selected) return;

    if (kind === 'edenLoad') {
      pcLoadDir = selected;
      loadDirEdited = true;
    }
    if (kind === 'prodKeys') prodKeysPath = selected;
    if (kind === 'packageLibrary') packageLibraryPath = selected;
  }

  /** @param {SubmitEvent} event */
  async function submitDesktop(event) {
    event.preventDefault();
    if (platform !== 'desktop' || componentBusy) return;
    if (!pcLoadDir.trim()) {
      localError = 'Select the Eden load directory before continuing.';
      return;
    }
    if (!onsubmit) {
      localError = 'Setup cannot be saved right now.';
      return;
    }

    await runAction('submit', () => onsubmit({
      ...settings,
      pcLoadDir: pcLoadDir.trim(),
      prodKeysPath: prodKeysPath.trim(),
      packageLibraryPath: packageLibraryPath.trim(),
      apiToken: apiToken.trim(),
      onboardingDone: true,
    }));
  }

  async function pickAndroid() {
    await runAction('android-picker', async () => {
      if (!onpickandroid) throw new Error('The Android folder picker is unavailable.');
      await onpickandroid();
    });
  }

  async function retryAndroid() {
    await runAction('android-refresh', async () => {
      if (!onretryandroid) throw new Error('Status refresh is unavailable.');
      await onretryandroid();
    });
  }
</script>

<main class="setup-screen">
  <section class="setup-card md-card md-card--elevated" aria-labelledby="setup-title" aria-busy={componentBusy}>
    <header class="hero">
      <div class="hero-icon" aria-hidden="true">
        <Icon name={platform === 'android' ? 'folder' : 'settings'} size={32} />
      </div>
      <div>
        <p class="eyebrow">Eden Cheats Manager</p>
        <h1 id="setup-title">
          {platform === 'android' ? 'Connect Eden storage' : 'Finish setting up'}
        </h1>
        <p class="subtitle">
          {#if platform === 'android'}
            Grant access to Eden's load directory before managing cheats on this device.
          {:else}
            Choose the local Eden folders ECM should use. You can change these later.
          {/if}
        </p>
      </div>
    </header>

    {#if componentBusy}
      <div class="md-progress" role="progressbar" aria-label="Setup in progress"></div>
    {/if}

    {#if displayedError}
      <div class="error-banner" role="alert">
        <Icon name="warning" size={20} />
        <span>{displayedError}</span>
      </div>
    {/if}

    {#if platform === 'desktop'}
      <form class="desktop-form" onsubmit={submitDesktop}>
        <section class="form-section" aria-labelledby="eden-storage-heading">
          <div class="section-heading">
            <Icon name="folder" size={24} />
            <div>
              <h2 id="eden-storage-heading">Eden storage</h2>
              <p>Required for finding installed games and managing their cheats.</p>
            </div>
          </div>

          <label class="md-field" for="setup-pc-load-dir">
            <span>Eden load directory</span>
            <span class="path-control">
              <input
                id="setup-pc-load-dir"
                bind:value={pcLoadDir}
                oninput={() => { loadDirEdited = true; }}
                required
                disabled={componentBusy}
                autocomplete="off"
                placeholder={detectedPcLoadDir || '/path/to/eden/load'}
              />
              <button
                class="md-icon-button md-icon-button--tonal"
                type="button"
                disabled={componentBusy}
                aria-label="Browse for Eden load directory"
                title="Browse for Eden load directory"
                onclick={() => pickDesktop('edenLoad', pcLoadDir)}
              >
                <Icon name="folder" size={24} />
              </button>
            </span>
            {#if detectedPcLoadDir}
              <span class="md-supporting-text">Detected: {detectedPcLoadDir}</span>
            {/if}
          </label>
        </section>

        <hr class="md-divider" />

        <section class="form-section" aria-labelledby="package-matching-heading">
          <div class="section-heading">
            <Icon name="key" size={24} />
            <div>
              <h2 id="package-matching-heading">Game library</h2>
              <p>
                Select prod.keys and your NSP/XCI folder to list games and detect their builds.
                You can connect them later in Settings; the library stays empty until then.
              </p>
            </div>
          </div>

          <div class="field-grid">
            <label class="md-field" for="setup-prod-keys">
              <span>prod.keys file</span>
              <span class="path-control">
                <input
                  id="setup-prod-keys"
                  bind:value={prodKeysPath}
                  disabled={componentBusy}
                  autocomplete="off"
                  placeholder="/path/to/prod.keys"
                />
                <button
                  class="md-icon-button md-icon-button--tonal"
                  type="button"
                  disabled={componentBusy}
                  aria-label="Browse for prod.keys"
                  title="Browse for prod.keys"
                  onclick={() => pickDesktop('prodKeys', prodKeysPath)}
                >
                  <Icon name="key" size={24} />
                </button>
              </span>
            </label>

            <label class="md-field" for="setup-package-library">
              <span>NSP/XCI library directory</span>
              <span class="path-control">
                <input
                  id="setup-package-library"
                  bind:value={packageLibraryPath}
                  disabled={componentBusy}
                  autocomplete="off"
                  placeholder="/path/to/packages"
                />
                <button
                  class="md-icon-button md-icon-button--tonal"
                  type="button"
                  disabled={componentBusy}
                  aria-label="Browse for package-library directory"
                  title="Browse for package-library directory"
                  onclick={() => pickDesktop('packageLibrary', packageLibraryPath)}
                >
                  <Icon name="folder" size={24} />
                </button>
              </span>
            </label>
          </div>

          {#if packagePairIncomplete}
            <p class="pair-note" role="status">
              <Icon name="info" size={20} />
              Scanning your game library needs both paths.
            </p>
          {/if}
        </section>

        <hr class="md-divider" />

        <section class="form-section" aria-labelledby="online-cheats-heading">
          <div class="section-heading">
            <Icon name="download" size={24} />
            <div>
              <h2 id="online-cheats-heading">Online cheats</h2>
              <p>Add a Cheatslips token to download cheats, or leave it blank for now.</p>
            </div>
          </div>

          <label class="md-field" for="setup-api-token">
            <span>API token <span class="optional">(optional)</span></span>
            <input
              id="setup-api-token"
              type="password"
              bind:value={apiToken}
              disabled={componentBusy}
              autocomplete="off"
              placeholder="Cheatslips API token"
            />
          </label>
        </section>

        <footer class="form-actions">
          <button class="md-button md-button--filled" type="submit" disabled={componentBusy}>
            <Icon name="check" size={20} />
            {activeAction === 'submit' || busy ? 'Saving…' : 'Finish setup'}
          </button>
        </footer>
      </form>
    {:else}
      <div class="android-content">
        <div class="status-card" class:ready={androidReady} role="status" aria-live="polite" aria-atomic="true">
          <span class="status-icon" aria-hidden="true">
            <Icon name={androidReady ? 'check' : 'folder'} size={24} />
          </span>
          <div>
            <h2>{androidReady ? 'Eden storage is ready' : 'Eden storage access required'}</h2>
            <p>{edenAccess?.message || 'Select Eden → load in Android’s folder picker.'}</p>
          </div>
        </div>

        <div class="android-guide">
          <h2>Choose the correct folder</h2>
          <ol>
            <li>Open the folder picker and choose <strong>Eden</strong> as the storage provider.</li>
            <li>Open <strong>load</strong>, then confirm with <strong>Use this folder</strong>.</li>
          </ol>
        </div>

        <div class="android-actions">
          <button class="md-button md-button--filled" type="button" disabled={componentBusy} onclick={pickAndroid}>
            <Icon name="folder" size={20} />
            {androidReady ? 'Choose again' : 'Select Eden → load'}
          </button>
          <button class="md-button md-button--outlined" type="button" disabled={componentBusy} onclick={retryAndroid}>
            <Icon name="refresh" size={20} />
            Check again
          </button>
        </div>
      </div>
    {/if}
  </section>
</main>

<style>
  .setup-screen {
    box-sizing: border-box;
    width: 100%;
    height: 100dvh;
    overflow-y: auto;
    overscroll-behavior: contain;
    padding:
      max(1rem, env(safe-area-inset-top))
      max(1rem, env(safe-area-inset-right))
      max(1rem, env(safe-area-inset-bottom))
      max(1rem, env(safe-area-inset-left));
    color: var(--md-sys-color-on-background);
    background:
      radial-gradient(circle at 50% 0%, color-mix(in srgb, var(--md-sys-color-primary) 10%, transparent), transparent 34rem),
      var(--md-sys-color-background);
  }

  .setup-card {
    width: min(100%, 58rem);
    margin: 0 auto;
    overflow: hidden;
  }

  .hero {
    display: flex;
    align-items: flex-start;
    gap: 1rem;
    padding: clamp(1.25rem, 4vw, 2rem);
    background: var(--md-sys-color-surface-container);
  }

  .hero-icon {
    display: grid;
    width: 3.5rem;
    height: 3.5rem;
    flex: 0 0 3.5rem;
    place-items: center;
    border-radius: var(--md-sys-shape-corner-large);
    color: var(--md-sys-color-on-primary-container);
    background: var(--md-sys-color-primary-container);
  }

  .eyebrow,
  h1,
  h2,
  p {
    margin: 0;
  }

  .eyebrow {
    color: var(--md-sys-color-primary);
    font-size: var(--md-sys-typescale-label-large-size);
    font-weight: 500;
    letter-spacing: 0.04em;
  }

  h1 {
    margin-top: 0.2rem;
    font-size: clamp(1.5rem, 5vw, var(--md-sys-typescale-headline-medium-size));
    line-height: 1.2;
  }

  .subtitle {
    max-width: 42rem;
    margin-top: 0.5rem;
    color: var(--md-sys-color-on-surface-variant);
    font-size: var(--md-sys-typescale-body-medium-size);
    line-height: 1.45;
  }

  .error-banner,
  .pair-note {
    display: flex;
    align-items: flex-start;
    gap: 0.75rem;
  }

  .error-banner {
    margin: 1rem clamp(1rem, 4vw, 2rem) 0;
    padding: 0.875rem 1rem;
    border-radius: var(--md-sys-shape-corner-medium);
    color: var(--md-sys-color-on-error-container);
    background: var(--md-sys-color-error-container);
    font-size: var(--md-sys-typescale-body-medium-size);
    line-height: 1.4;
  }

  .desktop-form,
  .android-content {
    display: grid;
    gap: 1.25rem;
    padding: clamp(1rem, 4vw, 2rem);
  }

  .form-section {
    display: grid;
    gap: 1rem;
  }

  .section-heading {
    display: flex;
    align-items: flex-start;
    gap: 0.75rem;
    color: var(--md-sys-color-primary);
  }

  .section-heading h2,
  .status-card h2,
  .android-guide h2 {
    color: var(--md-sys-color-on-surface);
    font-size: var(--md-sys-typescale-title-medium-size);
    font-weight: 500;
    line-height: 1.3;
  }

  .section-heading p,
  .status-card p {
    margin-top: 0.25rem;
    color: var(--md-sys-color-on-surface-variant);
    font-size: var(--md-sys-typescale-body-small-size);
    line-height: 1.45;
  }

  .field-grid {
    display: grid;
    gap: 1rem;
  }

  .path-control {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .path-control input {
    min-width: 0;
    flex: 1;
  }

  .optional {
    color: var(--md-sys-color-outline);
    font-weight: 400;
  }

  .pair-note {
    color: var(--md-sys-color-on-secondary-container);
    font-size: var(--md-sys-typescale-body-small-size);
    line-height: 1.4;
  }

  .form-actions {
    display: flex;
    justify-content: flex-end;
    padding-top: 0.25rem;
  }

  .status-card {
    display: flex;
    align-items: center;
    gap: 1rem;
    padding: 1rem;
    border: 1px solid var(--md-sys-color-outline-variant);
    border-radius: var(--md-sys-shape-corner-large);
    background: var(--md-sys-color-surface-container-highest);
  }

  .status-card.ready {
    border-color: var(--md-sys-color-tertiary);
  }

  .status-icon {
    display: grid;
    width: 3rem;
    height: 3rem;
    flex: 0 0 3rem;
    place-items: center;
    border-radius: var(--md-sys-shape-corner-full);
    color: var(--md-sys-color-on-secondary-container);
    background: var(--md-sys-color-secondary-container);
  }

  .status-card.ready .status-icon {
    color: var(--md-sys-color-on-tertiary-container);
    background: var(--md-sys-color-tertiary-container);
  }

  .android-guide {
    padding: 1rem;
    border-radius: var(--md-sys-shape-corner-large);
    background: var(--md-sys-color-surface-container);
  }

  .android-guide ol {
    margin: 0.75rem 0 0;
    padding-left: 1.4rem;
    color: var(--md-sys-color-on-surface-variant);
    font-size: var(--md-sys-typescale-body-medium-size);
    line-height: 1.55;
  }

  .android-guide li + li {
    margin-top: 0.5rem;
  }

  .android-actions {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: 0.75rem;
  }

  @media (min-width: 700px) {
    .field-grid {
      grid-template-columns: 1fr 1fr;
    }
  }

  @media (max-width: 520px) {
    .hero {
      padding: 1.25rem 1rem;
    }

    .hero-icon {
      width: 3rem;
      height: 3rem;
      flex-basis: 3rem;
    }

    .desktop-form,
    .android-content {
      padding: 1rem;
    }

    .android-actions,
    .form-actions {
      display: grid;
    }

    .android-actions .md-button,
    .form-actions .md-button {
      width: 100%;
    }
  }

  @media (max-height: 500px) and (min-width: 600px) {
    .setup-screen {
      padding-block: max(0.5rem, env(safe-area-inset-top)) max(0.5rem, env(safe-area-inset-bottom));
    }

    .hero {
      padding-block: 1rem;
    }

    .desktop-form,
    .android-content {
      gap: 1rem;
      padding-block: 1rem;
    }
  }
</style>
