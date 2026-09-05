<script>
  import Icon from './ui/Icon.svelte';
  import Dialog from './ui/Dialog.svelte';
  import { theme } from '../domain/theme.js';

  /** @typedef {import('../api/types.js').AppSettings} AppSettings */
  /** @typedef {import('../api/types.js').EdenLoadAccessStatus} EdenLoadAccessStatus */
  /** @typedef {import('../api/types.js').GameLibraryStatus} GameLibraryStatus */
  /** @typedef {import('../api/types.js').PackageDiscoveryStatus} PackageDiscoveryStatus */
  /** @typedef {import('../api/types.js').Platform} Platform */
  /** @typedef {'edenLoad' | 'prodKeys' | 'packageLibrary'} SettingsPickerKind */

  /**
   * @typedef {Object} PendingPicker
   * @property {number} id
   * @property {'edenLoad' | 'prodKeys' | 'packageLibrary' | 'singlePackage'} kind
   */

  /**
   * @typedef {Object} Props
   * @property {boolean} [open]
   * @property {Platform} [platform]
   * @property {AppSettings} settings
   * @property {EdenLoadAccessStatus | null} [edenAccess]
   * @property {PackageDiscoveryStatus | null} [packageStatus]
   * @property {GameLibraryStatus | null} [gameLibraryStatus]
   * @property {PendingPicker | null} [pendingPicker]
   * @property {boolean} [saving]
   * @property {string} [error]
   * @property {(settings: AppSettings) => void | Promise<void>} [onsave]
   * @property {() => void} onclose
   * @property {(kind: SettingsPickerKind, currentPath: string) => string | null | Promise<string | null>} [onpickdesktop]
   * @property {(kind: SettingsPickerKind) => void | Promise<void>} [onpickandroid]
   * @property {() => void | Promise<void>} [onretryandroid]
   * @property {() => string | Promise<string>} [ontesteden]
   * @property {() => boolean | Promise<boolean>} [onrevealapplog]
   */

  /** @type {Props} */
  let {
    open = false,
    platform = 'desktop',
    settings,
    edenAccess = null,
    packageStatus = null,
    gameLibraryStatus = null,
    pendingPicker = null,
    saving = false,
    error = '',
    onsave,
    onclose,
    onpickdesktop,
    onpickandroid,
    onretryandroid,
    ontesteden,
    onrevealapplog,
  } = $props();

  /** @param {AppSettings | null | undefined} value @returns {AppSettings} */
  function copySettings(value) {
    return {
      ...(value ?? {}),
      apiToken: value?.apiToken ?? '',
      pcLoadDir: value?.pcLoadDir ?? '',
      prodKeysPath: value?.prodKeysPath ?? '',
      packageLibraryPath: value?.packageLibraryPath ?? '',
      edenExePath: value?.edenExePath ?? '',
      onboardingDone: value?.onboardingDone ?? false,
    };
  }

  let draft = $state(/** @type {AppSettings} */ (copySettings(null)));
  let activeAction = $state('');
  let localError = $state('');
  let testResult = $state('');
  let logResult = $state('');
  let submitting = $state(false);
  let wasOpen = false;
  let session = 0;

  $effect(() => {
    if (open !== wasOpen) session++;
    if (open && !wasOpen) {
      draft = copySettings(settings);
      activeAction = '';
      localError = '';
      testResult = '';
      logResult = '';
    }
    wasOpen = open;
  });

  let isDesktop = $derived(platform === 'desktop');
  let keysReady = $derived(Boolean(
    packageStatus?.prodKeysSelected
      && packageStatus.prodKeysReadable
      && packageStatus.prodKeysSeekable,
  ));
  let packagePairIncomplete = $derived(
    isDesktop
      && Boolean(draft.prodKeysPath.trim()) !== Boolean(draft.packageLibraryPath.trim()),
  );
  let loadDirectoryMissing = $derived(isDesktop && !draft.pcLoadDir.trim());
  let displayedError = $derived(localError || error);
  let busy = $derived(saving || submitting || Boolean(activeAction) || Boolean(pendingPicker));

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
    if (activeAction) return undefined;
    const revision = session;
    activeAction = name;
    localError = '';
    try {
      const result = await action();
      return revision === session && open ? result : undefined;
    } catch (cause) {
      if (revision === session && open) localError = errorMessage(cause);
      return undefined;
    } finally {
      if (revision === session) activeAction = '';
    }
  }

  /** @param {SettingsPickerKind} kind @param {keyof Pick<AppSettings, 'pcLoadDir' | 'prodKeysPath' | 'packageLibraryPath'>} field */
  async function browseDesktop(kind, field) {
    const selected = await runAction(`desktop-${kind}`, async () => {
      if (!onpickdesktop) throw new Error('The desktop picker is unavailable.');
      return onpickdesktop(kind, draft[field]);
    });
    if (typeof selected === 'string' && selected) draft[field] = selected;
  }

  /** @param {SettingsPickerKind} kind */
  async function pickAndroid(kind) {
    await runAction(`android-${kind}`, async () => {
      if (!onpickandroid) throw new Error('The Android picker is unavailable.');
      await onpickandroid(kind);
    });
  }

  async function retryAndroid() {
    await runAction('android-refresh', async () => {
      if (!onretryandroid) throw new Error('Status refresh is unavailable.');
      await onretryandroid();
    });
  }

  async function testAndroidEden() {
    testResult = '';
    const result = await runAction('android-test', async () => {
      if (!ontesteden) throw new Error('The Eden access test is unavailable.');
      return ontesteden();
    });
    if (typeof result === 'string') testResult = result;
  }

  async function revealAppLog() {
    logResult = '';
    const revealed = await runAction('desktop-log', async () => {
      if (!onrevealapplog) throw new Error('The app log is unavailable.');
      return onrevealapplog();
    });
    if (typeof revealed === 'boolean') {
      logResult = revealed ? 'Opened the app log.' : 'The app log is not available yet.';
    }
  }

  async function submit() {
    if (busy) return;
    const revision = session;
    localError = '';
    if (loadDirectoryMissing) {
      localError = 'Select the Eden load directory before saving.';
      return;
    }
    if (!onsave) {
      localError = 'Settings cannot be saved right now.';
      return;
    }

    submitting = true;
    try {
      await onsave(copySettings(draft));
    } catch (cause) {
      if (revision === session && open) localError = errorMessage(cause);
    } finally {
      submitting = false;
    }
  }

  function requestClose() {
    onclose();
  }

  function prodKeysMessage() {
    if (!packageStatus) return 'Prod.keys status has not been checked.';
    if (!packageStatus.prodKeysSelected) return 'Select prod.keys to match package builds.';
    if (!packageStatus.prodKeysReadable) return 'Android can no longer read prod.keys.';
    if (!packageStatus.prodKeysSeekable) return 'The selected provider is not seekable; choose a local file.';
    return 'Prod.keys is ready.';
  }
</script>

<Dialog {open} fullScreen title="Settings" onclose={requestClose}>
  <form
    id="settings-form"
    class="settings-form"
    aria-busy={busy}
    novalidate
    onsubmit={(event) => {
      event.preventDefault();
      submit();
    }}
  >
    <section class="settings-section" aria-labelledby="settings-appearance-title">
      <div class="section-heading">
        <Icon name="palette" />
        <div>
          <h3 id="settings-appearance-title">Appearance</h3>
          <p>Choose a theme, or follow your device. Saved automatically.</p>
        </div>
      </div>
      <div class="theme-options" role="radiogroup" aria-label="Colour theme">
        {#each /** @type {const} */ (['system', 'light', 'dark']) as choice}
          <label class="theme-option" class:selected={$theme === choice}>
            <input type="radio" name="theme" value={choice} bind:group={$theme} />
            <Icon name={$theme === choice ? 'check' : choice === 'system' ? 'contrast' : choice === 'light' ? 'light_mode' : 'dark_mode'} />
            {choice === 'system' ? 'System' : choice === 'light' ? 'Light' : 'Dark'}
          </label>
        {/each}
      </div>
    </section>
    {#if isDesktop}
      <section class="settings-section" aria-labelledby="settings-storage-title">
        <div class="section-heading">
          <Icon name="folder" size={24} />
          <div>
            <h3 id="settings-storage-title">Eden storage</h3>
            <p>ECM installs cheat files in Eden's load directory.</p>
          </div>
        </div>

        <div class="md-field">
          <label for="settings-load-directory">Eden load directory</label>
          <div class="path-control">
            <input
              id="settings-load-directory"
              bind:value={draft.pcLoadDir}
              placeholder="Select Eden's load directory"
              spellcheck="false"
              aria-invalid={loadDirectoryMissing}
              aria-describedby={loadDirectoryMissing ? 'settings-load-directory-error' : undefined}
            />
            <button
              class="md-icon-button"
              type="button"
              aria-label="Browse for Eden load directory"
              disabled={busy}
              onclick={() => browseDesktop('edenLoad', 'pcLoadDir')}
            >
              <Icon name="folder" />
            </button>
          </div>
          {#if loadDirectoryMissing}
            <span id="settings-load-directory-error" class="md-error-text">
              Required to list, install, and remove cheats.
            </span>
          {/if}
        </div>
      </section>

      <section class="settings-section" aria-labelledby="settings-packages-title">
        <div class="section-heading">
          <Icon name="game" size={24} />
          <div>
            <h3 id="settings-packages-title">Game library</h3>
            <p>Scan NSP/XCI games and updates to populate your library and detect builds.</p>
          </div>
        </div>

        <div class="md-field">
          <label for="settings-prod-keys">Prod.keys</label>
          <div class="path-control">
            <input
              id="settings-prod-keys"
              bind:value={draft.prodKeysPath}
              placeholder="Select prod.keys"
              spellcheck="false"
              aria-invalid={packagePairIncomplete}
              aria-describedby={packagePairIncomplete ? 'settings-package-pair-error' : undefined}
            />
            <button
              class="md-icon-button"
              type="button"
              aria-label="Browse for prod.keys"
              disabled={busy}
              onclick={() => browseDesktop('prodKeys', 'prodKeysPath')}
            >
              <Icon name="key" />
            </button>
          </div>
        </div>

        <div class="md-field">
          <label for="settings-package-library">Game-package library</label>
          <div class="path-control">
            <input
              id="settings-package-library"
              bind:value={draft.packageLibraryPath}
              placeholder="Select a folder containing NSP or XCI files"
              spellcheck="false"
              aria-invalid={packagePairIncomplete}
              aria-describedby={packagePairIncomplete ? 'settings-package-pair-error' : undefined}
            />
            <button
              class="md-icon-button"
              type="button"
              aria-label="Browse for game-package library"
              disabled={busy}
              onclick={() => browseDesktop('packageLibrary', 'packageLibraryPath')}
            >
              <Icon name="folder" />
            </button>
          </div>
        </div>

        {#if packagePairIncomplete}
          <p id="settings-package-pair-error" class="md-error-text" role="alert">
            Bulk matching needs both paths. A keys file alone can be used to inspect one package.
          </p>
        {/if}
      </section>
    {:else}
      <section class="settings-section" aria-labelledby="settings-android-storage-title">
        <div class="section-heading">
          <Icon name="folder" size={24} />
          <div>
            <h3 id="settings-android-storage-title">Eden storage</h3>
            <p>Read and write access to Eden → load.</p>
          </div>
        </div>

        <div class="access-entry">
          <span class:ready={edenAccess?.ready} class="status-icon" aria-hidden="true">
            <Icon name={edenAccess?.ready ? 'check' : 'warning'} size={20} />
          </span>
          <div class="status-copy">
            <strong>{edenAccess?.ready ? 'Eden access ready' : 'Eden access required'}</strong>
            <span>{edenAccess?.message ?? 'Eden access has not been checked.'}</span>
          </div>
          <button
            class="md-button md-button--outlined status-action"
            type="button"
            disabled={busy || pendingPicker?.kind === 'edenLoad'}
            onclick={() => pickAndroid('edenLoad')}
          >
            {pendingPicker?.kind === 'edenLoad' ? 'Waiting…' : 'Select folder'}
          </button>
        </div>

        <div class="inline-actions">
          <button
            class="md-button md-button--text"
            type="button"
            disabled={busy || !edenAccess?.ready}
            onclick={testAndroidEden}
          >
            Test access
          </button>
          <button
            class="md-button md-button--text"
            type="button"
            disabled={busy}
            onclick={retryAndroid}
          >
            <Icon name="refresh" size={20} />
            Refresh status
          </button>
        </div>

        {#if testResult}
          <p
            class:success={testResult === 'OK'}
            class:error-result={testResult !== 'OK'}
            class="action-result"
            role="status"
            aria-live="polite"
          >
            {testResult === 'OK' ? 'Eden read and write access works.' : testResult}
          </p>
        {/if}
      </section>

      <section class="settings-section" aria-labelledby="settings-android-packages-title">
        <div class="section-heading">
          <Icon name="game" size={24} />
          <div>
            <h3 id="settings-android-packages-title">Game library</h3>
            <p>Connect prod.keys and your NSP/XCI folder to populate the library.</p>
          </div>
        </div>

        <div class="access-entry">
          <span class:ready={keysReady} class="status-icon" aria-hidden="true">
            <Icon name={keysReady ? 'check' : 'warning'} size={20} />
          </span>
          <div class="status-copy">
            <strong>{packageStatus?.prodKeysName || 'Prod.keys'}</strong>
            <span>{prodKeysMessage()}</span>
          </div>
          <button
            class="md-button md-button--outlined status-action"
            type="button"
            disabled={busy || pendingPicker?.kind === 'prodKeys'}
            onclick={() => pickAndroid('prodKeys')}
          >
            {pendingPicker?.kind === 'prodKeys' ? 'Waiting…' : 'Select file'}
          </button>
        </div>

        <hr class="md-divider" />

        <div class="access-entry">
          <span class:ready={gameLibraryStatus?.ready} class="status-icon" aria-hidden="true">
            <Icon name={gameLibraryStatus?.ready ? 'check' : 'warning'} size={20} />
          </span>
          <div class="status-copy">
            <strong>{gameLibraryStatus?.name || 'Game-package library'}</strong>
            <span>{gameLibraryStatus?.message ?? 'Package-library access has not been checked.'}</span>
          </div>
          <button
            class="md-button md-button--outlined status-action"
            type="button"
            disabled={busy || pendingPicker?.kind === 'packageLibrary'}
            onclick={() => pickAndroid('packageLibrary')}
          >
            {pendingPicker?.kind === 'packageLibrary' ? 'Waiting…' : 'Select folder'}
          </button>
        </div>
      </section>

      <p class="md-sr-only" role="status" aria-live="polite" aria-atomic="true">
        Eden: {edenAccess?.message ?? 'not checked'}
        Prod.keys: {prodKeysMessage()}
        Package library: {gameLibraryStatus?.message ?? 'not checked'}
      </p>
    {/if}

    <section class="settings-section" aria-labelledby="settings-online-title">
      <div class="section-heading">
        <Icon name="download" size={24} />
        <div>
          <h3 id="settings-online-title">Online cheat source</h3>
          <p>Optional token used only when you request an online fetch.</p>
        </div>
      </div>

      <div class="md-field">
        <label for="settings-api-token">Cheatslips API token</label>
        <input
          id="settings-api-token"
          type="password"
          bind:value={draft.apiToken}
          autocomplete="off"
          placeholder="API token"
          spellcheck="false"
        />
      </div>
    </section>

    {#if isDesktop}
      <section class="settings-section" aria-labelledby="settings-diagnostics-title">
        <div class="section-heading">
          <Icon name="info" size={24} />
          <div>
            <h3 id="settings-diagnostics-title">Diagnostics</h3>
            <p>Reveal ECM's local application log.</p>
          </div>
        </div>

        <button
          class="md-button md-button--outlined section-button"
          type="button"
          disabled={busy}
          onclick={revealAppLog}
        >
          Open app log
        </button>
        {#if logResult}
          <p class="action-result" role="status" aria-live="polite">{logResult}</p>
        {/if}
      </section>
    {/if}

    {#if displayedError}
      <p class="dialog-error md-error-text" role="alert">{displayedError}</p>
    {/if}
  </form>

  {#snippet actions()}
    <button class="md-button md-button--text" type="button" onclick={requestClose}>
      Cancel
    </button>
    <button
      class="md-button md-button--filled"
      type="submit"
      form="settings-form"
      disabled={busy}
    >
      {saving || submitting ? 'Saving…' : 'Save'}
    </button>
  {/snippet}
</Dialog>

<style>
  .theme-options { display: flex; }
  .theme-option {
    display: flex; align-items: center; justify-content: center; gap: 0.5rem;
    position: relative; flex: 1; min-width:0; min-height: var(--md-sys-size-touch); padding: 0.5rem;
    border: 1px solid var(--md-sys-color-outline);
    font:var(--md-sys-typescale-label-large);
    color: var(--md-sys-color-on-surface); cursor: pointer;
  }
  .theme-option + .theme-option { border-inline-start:0; }
  .theme-option:first-child { border-radius:24px 0 0 24px; }
  .theme-option:last-child { border-radius:0 24px 24px 0; }
  .theme-option.selected { background: var(--md-sys-color-secondary-container); color: var(--md-sys-color-on-secondary-container); }
  .theme-option:has(input:focus-visible) { outline: 2px solid var(--md-sys-color-primary); outline-offset: 2px; z-index:1; }
  .theme-option input { position: absolute; opacity: 0; width: 1px; height: 1px; }
  .theme-option:hover { box-shadow: inset 0 0 0 100px rgb(var(--md-sys-color-on-surface-rgb) / 0.08); }

  .settings-form {
    display: grid;
    gap: 1.5rem;
  }

  .settings-section {
    display: grid;
    gap: 1rem;
    padding: 0 0 1.5rem;
    border-bottom:1px solid var(--md-sys-color-outline-variant);
  }
  .settings-section:last-of-type { border-bottom:0; padding-bottom:0; }

  .section-heading {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr);
    align-items: start;
    gap: 0.75rem;
    color: var(--md-sys-color-primary);
  }

  .section-heading h3 {
    color: var(--md-sys-color-on-surface);
    font-size: var(--md-sys-typescale-title-medium-size);
    font-weight: 500;
    line-height: 1.5rem;
  }

  .section-heading p {
    margin-top: 0.125rem;
    color: var(--md-sys-color-on-surface-variant);
    font-size: var(--md-sys-typescale-body-small-size);
    line-height: 1rem;
  }

  .path-control {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    align-items: center;
    gap: 0.5rem;
  }

  .path-control input {
    min-width: 0;
  }

  .access-entry {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr);
    align-items: start;
    gap: 0.75rem;
  }

  .status-icon {
    display: grid;
    width: 2rem;
    height: 2rem;
    place-items: center;
    border-radius: var(--md-sys-shape-corner-full);
    color: var(--md-sys-color-on-error-container);
    background: var(--md-sys-color-error-container);
  }

  .status-icon.ready {
    color: var(--md-sys-color-on-secondary-container);
    background: var(--md-sys-color-secondary-container);
  }

  .status-copy {
    display: grid;
    min-width: 0;
    gap: 0.125rem;
  }

  .status-copy strong {
    overflow-wrap: anywhere;
    color: var(--md-sys-color-on-surface);
    font-size: var(--md-sys-typescale-body-large-size);
    font-weight: 500;
    line-height: 1.5rem;
  }

  .status-copy span {
    overflow-wrap: anywhere;
    color: var(--md-sys-color-on-surface-variant);
    font-size: var(--md-sys-typescale-body-small-size);
    line-height: 1rem;
  }

  .status-action {
    grid-column: 2;
    justify-self: start;
  }

  .inline-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 0.25rem;
  }

  .section-button {
    justify-self: start;
  }

  .action-result {
    color: var(--md-sys-color-on-surface-variant);
    font-size: var(--md-sys-typescale-body-small-size);
    line-height: 1rem;
  }

  .action-result.success {
    color: var(--md-sys-color-on-surface-variant);
  }

  .action-result.error-result,
  .dialog-error {
    color: var(--md-sys-color-error);
  }

  .dialog-error {
    padding: 0.75rem 1rem;
    border-radius: var(--md-sys-shape-corner-small);
    background: var(--md-sys-color-error-container);
    color: var(--md-sys-color-on-error-container);
  }

  @media (min-width: 30rem) {
    .access-entry {
      grid-template-columns: auto minmax(0, 1fr) auto;
      align-items: center;
    }

    .status-action {
      grid-column: auto;
      justify-self: end;
    }
  }

  @media (max-height: 599px) {
    .settings-form,
    .settings-section {
      gap: 0.75rem;
    }

    .settings-section {
      padding-bottom: 1rem;
    }
  }
</style>
