<script>
  import Icon from './ui/Icon.svelte';

  /** @typedef {import('../api/types.js').GameGroup} GameGroup */
  /** @typedef {import('../api/types.js').TitleEntry} TitleEntry */
  /** @typedef {'idle' | 'loading' | 'error'} RefreshPhase */

  /**
   * @typedef {Object} Props
   * @property {GameGroup[]} games
   * @property {RefreshPhase} refreshPhase
   * @property {import('../api/types.js').ManagedPackageLibrary|null} packageLibrary
   * @property {string} refreshError
   * @property {string} selectedTitleId
   * @property {(titleId: string) => void} onselect
   * @property {() => void} onrefresh
   * @property {() => void} onsettings
   */

  /** @type {Props} */
  let {
    games,
    refreshPhase,
    packageLibrary,
    refreshError,
    selectedTitleId,
    onselect,
    onrefresh,
    onsettings,
  } = $props();

  const id = $props.id();
  const searchId = `${id}-search`;
  let query = $state('');

  /**
   * @param {GameGroup} group
   * @returns {{ entry: TitleEntry, kind: string }[]}
   */
  function entriesForGroup(group) {
    return [
      ...(group.baseGame
        ? [{ entry: group.baseGame, kind: 'Base game' }]
        : []),
      ...group.updates.map((entry) => ({ entry, kind: 'Update' })),
    ];
  }

  /** @param {GameGroup} group @param {string} term */
  function groupMatches(group, term) {
    const values = [
      group.baseName,
      group.baseTitleId,
      ...entriesForGroup(group).flatMap(({ entry, kind }) => [entry.name, entry.titleId, kind]),
    ];
    return values.some((value) => value.toLowerCase().includes(term));
  }

  let normalizedQuery = $derived(query.trim().toLowerCase());
  let filteredGames = $derived.by(() => normalizedQuery
    ? games.filter((group) => groupMatches(group, normalizedQuery))
    : games);
  let refreshing = $derived(refreshPhase === 'loading');
  let needsSetup = $derived(packageLibrary?.state === 'notConfigured');
  let libraryError = $derived(refreshError || (packageLibrary?.state === 'error' ? packageLibrary.message : '')
    || (packageLibrary?.state === 'ready' && !games.length && packageLibrary.correlation.packageScanErrors.length
      ? `No games could be read. ${packageLibrary.correlation.packageScanErrors[0].message}` : ''));
  let groupCountLabel = $derived(
    `${games.length} game${games.length === 1 ? '' : 's'}`,
  );
</script>

<aside class="library-pane" aria-labelledby={`${id}-title`}>
  <header class="top-app-bar">
    <div class="title-block">
      <img class="brand" src="/edc-logo.png" alt="EDC" width="40" height="40" />
      <div>
        <h1 id={`${id}-title`}>Game library</h1>
        <p>{groupCountLabel}</p>
      </div>
    </div>

    <div class="top-actions">
      <button
        type="button"
        class="md-icon-button"
        aria-label={refreshing ? 'Refreshing game library' : 'Refresh game library'}
        disabled={refreshing}
        onclick={onrefresh}
      >
        <span class:spinning={refreshing}><Icon name="refresh" /></span>
      </button>
      <button
        type="button"
        class="md-icon-button"
        aria-label="Open settings"
        onclick={onsettings}
      >
        <Icon name="settings" />
      </button>
    </div>
  </header>

  <div class="library-scroll" aria-busy={refreshing}>
    <div class="md-field search-field">
      <label class="md-sr-only" for={searchId}>Search games</label>
      <div class="search-control">
        <span class="search-icon" aria-hidden="true"><Icon name="search" /></span>
        <input
          id={searchId}
          type="search"
          bind:value={query}
          placeholder="Search games"
          autocomplete="off"
          spellcheck="false"
        />
        {#if query}
          <button type="button" class="md-icon-button search-clear" aria-label="Clear search" onclick={() => query = ''}><Icon name="close" size={20} /></button>
        {/if}
      </div>
    </div>

    {#if refreshing}
      <div class="refresh-status" role="status" aria-live="polite">
        <div class="md-progress" role="progressbar" aria-label="Refreshing game library"></div>
        <span>Scanning game packages…</span>
      </div>
    {:else if libraryError}
      <div class="refresh-error" role="alert">
        <Icon name="warning" size={20} />
        <span>{libraryError}</span>
      </div>
    {/if}

    {#if games.length === 0 && !refreshing}
      <div class="empty-state" role="status">
        <span class="empty-icon" aria-hidden="true"><Icon name="game" size={32} /></span>
        <h2>{needsSetup ? 'Connect your game library' : libraryError ? 'Unable to load game library' : 'No games found'}</h2>
        <p>{needsSetup ? 'Select prod.keys and the folder containing your NSP/XCI games in Settings.'
          : libraryError ? 'Check your package folder and prod.keys, then try again.'
          : 'Add base games or updates to your NSP/XCI folder, then refresh. DLC is not listed.'}</p>
        <button type="button" class="md-button md-button--tonal" onclick={onsettings}>Open Settings</button>
        {#if !needsSetup}
        <button type="button" class="md-button md-button--tonal" onclick={onrefresh}>
          <Icon name="refresh" size={20} />
          Refresh library
        </button>
        {/if}
      </div>
    {:else if !refreshing && normalizedQuery && filteredGames.length === 0}
      <div class="empty-state search-empty" role="status" aria-live="polite">
        <span class="empty-icon" aria-hidden="true"><Icon name="search" size={32} /></span>
        <h2>No matching games</h2>
        <p>No game name or Title ID matches “{query.trim()}”.</p>
        <button type="button" class="md-button md-button--text" onclick={() => query = ''}>
          Clear search
        </button>
      </div>
    {:else}
      <div class="game-grid" role="list" aria-label="Games">
        {#each filteredGames as group}
          <article class="game-card" role="listitem">
            <button type="button" class="game-card__header md-list-item"
              data-title-id={group.baseTitleId}
              aria-current={selectedTitleId === group.baseTitleId ? 'true' : undefined}
              onclick={() => onselect(group.baseTitleId)}>
              {#if group.baseImage}
                <img class="game-cover" src={group.baseImage} alt="" loading="lazy" decoding="async" />
              {:else}
                <span class="game-cover placeholder" aria-hidden="true">
                  <Icon name="game" size={32} />
                </span>
              {/if}

              <div class="game-heading">
                <h2>{group.baseName || group.baseTitleId}</h2>
                <code>{group.baseTitleId}</code>
                {#if group.baseInstalled || group.updates.some(entry => entry.installed)}
                  <span class="installed-label"><Icon name="check" size={20} /> Present in Eden</span>
                {:else}
                  <span class="absent-label">Not present in Eden</span>
                {/if}
              </div>
            </button>

          </article>
        {/each}
      </div>
    {/if}
  </div>
</aside>

<style>
  .library-pane {
    container: library / inline-size;
    display: flex;
    width: 100%;
    height: 100%;
    min-width: 0;
    min-height: 0;
    flex-direction: column;
    overflow: hidden;
    color: var(--md-sys-color-on-surface);
    background: var(--md-sys-color-surface);
  }

  .top-app-bar {
    display: flex;
    min-height: var(--md-sys-size-top-app-bar);
    flex: 0 0 auto;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
    padding-block-start: max(0.5rem, env(safe-area-inset-top));
    padding-block-end: 0.5rem;
    padding-inline: max(1rem, env(safe-area-inset-left)) max(1rem, env(safe-area-inset-right));
    background: var(--md-sys-color-surface);
  }

  .title-block {
    display: flex;
    min-width: 0;
    align-items: center;
    gap: 0.75rem;
  }

  .brand {
    flex: 0 0 auto;
    width: 2.5rem;
    height: 2.5rem;
  }

  .title-block > div {
    min-width: 0;
  }

  h1 {
    overflow-wrap: anywhere;
    color: var(--md-sys-color-on-surface);
    font-size: var(--md-sys-typescale-title-large-size);
    font-weight: 400;
    line-height: 1.75rem;
  }

  .title-block p {
    color: var(--md-sys-color-on-surface-variant);
    font-size: var(--md-sys-typescale-body-small-size);
    line-height: 1rem;
  }

  .top-actions {
    display: flex;
    flex: 0 0 auto;
    align-items: center;
  }

  .top-actions button span {
    display: block;
  }

  .spinning {
    animation: spin 0.9s linear infinite;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  .library-scroll {
    display: flex;
    min-height: 0;
    flex: 1;
    flex-direction: column;
    gap: 1rem;
    overflow: auto;
    overscroll-behavior: contain;
    padding: 1rem var(--md-sys-layout-gutter) max(1rem, env(safe-area-inset-bottom));
  }

  .search-field {
    flex: 0 0 auto;
  }

  .search-control {
    position: relative;
  }

  .search-control input {
    padding-inline: 3.5rem;
    border: 0;
    border-radius: var(--md-sys-shape-corner-full);
    background: var(--md-sys-color-surface-container-high);
  }

  .search-control input::-webkit-search-cancel-button { display: none; }
  .search-clear { position: absolute; inset-inline-end: 4px; inset-block-start: 4px; }

  .search-icon {
    position: absolute;
    z-index: 1;
    inset-block-start: 50%;
    inset-inline-start: 1rem;
    color: var(--md-sys-color-on-surface-variant);
    pointer-events: none;
    transform: translateY(-50%);
  }

  .refresh-status,
  .refresh-error {
    flex: 0 0 auto;
  }

  .refresh-status {
    display: grid;
    gap: 0.5rem;
    color: var(--md-sys-color-on-surface-variant);
    font-size: var(--md-sys-typescale-body-small-size);
  }

  .refresh-error {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr);
    align-items: start;
    gap: 0.75rem;
    padding: 0.75rem 1rem;
    border-radius: var(--md-sys-shape-corner-medium);
  }

  .refresh-error {
    color: var(--md-sys-color-on-error-container);
    background: var(--md-sys-color-error-container);
  }

  .refresh-error span {
    overflow-wrap: anywhere;
    font-size: var(--md-sys-typescale-body-small-size);
    line-height: 1rem;
  }

  .game-grid {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    align-items: stretch;
    gap: 0.5rem;
    padding-block-end: 0.5rem;
  }

  .game-card {
    min-width: 0;
    overflow: hidden;
    border-radius: var(--md-sys-shape-corner-medium);
  }

  .game-card__header {
    display: flex;
    min-width: 0;
    align-items: center;
    gap: 1rem;
    padding: 1rem;
    height: 100%;
    background: var(--md-sys-color-surface-container-low);
  }

  .game-card__header[aria-current="true"] {
    color: var(--md-sys-color-on-secondary-container);
    background: var(--md-sys-color-secondary-container);
  }

  .game-card__header:focus-visible { outline-offset: -2px; border-radius: inherit; }

  .game-cover {
    width: 4.5rem;
    height: 4.5rem;
    flex: 0 0 4.5rem;
    border-radius: var(--md-sys-shape-corner-medium);
    object-fit: cover;
    background: var(--md-sys-color-surface-container-highest);
  }

  .game-cover.placeholder {
    display: grid;
    place-items: center;
    color: var(--md-sys-color-on-surface-variant);
  }

  .game-heading {
    display: flex;
    min-width: 0;
    flex: 1;
    flex-direction: column;
    align-items: flex-start;
    gap: 0.25rem;
  }

  .game-heading h2,
  .empty-state p {
    overflow-wrap: anywhere;
  }

  .game-heading h2 {
    color: inherit;
    font: var(--md-sys-typescale-title-medium);
    letter-spacing: var(--md-sys-typescale-title-medium-tracking);
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }

  code {
    max-width: 100%;
    overflow-wrap: anywhere;
    color: var(--md-sys-color-on-surface-variant);
    font-family: inherit;
    font-size: var(--md-sys-typescale-label-medium-size);
    line-height: 1rem;
  }

  .installed-label {
    display: inline-flex;
    min-height: 1.5rem;
    align-items: center;
    gap: 0.25rem;
    color: var(--md-sys-color-on-surface-variant);
    font: var(--md-sys-typescale-body-small);
    white-space: nowrap;
  }

  .absent-label { color:var(--md-sys-color-on-surface-variant); font-size:12px; }

  .empty-state {
    display: flex;
    min-height: 15rem;
    flex: 1;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 0.75rem;
    padding: 2rem 1rem;
    color: var(--md-sys-color-on-surface-variant);
    text-align: center;
  }

  .empty-icon {
    display: grid;
    width: 4rem;
    height: 4rem;
    place-items: center;
    border-radius: var(--md-sys-shape-corner-full);
    color: var(--md-sys-color-primary);
    background: var(--md-sys-color-primary-container);
  }

  .empty-state h2 {
    color: var(--md-sys-color-on-surface);
    font-size: var(--md-sys-typescale-title-large-size);
    font-weight: 400;
    line-height: 1.75rem;
  }

  .empty-state p {
    max-width: 34rem;
    line-height: 1.5rem;
  }

  @container library (min-width: 600px) {
    .game-grid {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }

  @container library (max-width: 420px) {
    .library-scroll {
      padding-inline: 1rem;
    }

    .game-card__header {
      gap: 0.75rem;
      padding: 0.75rem;
    }

    .game-cover {
      width: 3.5rem;
      height: 3.5rem;
      flex-basis: 3.5rem;
    }

  }

  @media (max-height: 599px) {
    .top-app-bar {
      min-height: 3.5rem;
    }

    .library-scroll {
      gap: 0.75rem;
      padding-block-start: 0.75rem;
    }

    .game-grid {
      gap: 0.75rem;
    }

    .game-card__header {
      padding: 0.75rem;
    }
  }
</style>
