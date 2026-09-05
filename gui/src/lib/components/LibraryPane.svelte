<script>
  import Icon from './ui/Icon.svelte';

  /** @typedef {import('../api/types.js').GameGroup} GameGroup */
  /** @typedef {import('../api/types.js').TitleEntry} TitleEntry */
  /** @typedef {'idle' | 'loading' | 'error'} RefreshPhase */

  /**
   * @typedef {Object} Props
   * @property {GameGroup[]} games
   * @property {RefreshPhase} refreshPhase
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
   * @returns {{ entry: TitleEntry, kind: string, unsupported: boolean }[]}
   */
  function entriesForGroup(group) {
    return [
      ...(group.baseGame
        ? [{ entry: group.baseGame, kind: 'Base game', unsupported: false }]
        : []),
      ...group.updates.map((entry) => ({ entry, kind: 'Update', unsupported: false })),
      ...group.dlcs.map((entry) => ({ entry, kind: 'DLC', unsupported: true })),
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
  let groupCountLabel = $derived(
    `${games.length} game group${games.length === 1 ? '' : 's'}`,
  );
</script>

<aside class="library-pane" aria-labelledby={`${id}-title`}>
  <header class="top-app-bar">
    <div class="title-block">
      <span class="brand" aria-hidden="true">ECM</span>
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
      <label for={searchId}>Search games</label>
      <div class="search-control">
        <span class="search-icon" aria-hidden="true"><Icon name="search" size={20} /></span>
        <input
          id={searchId}
          type="search"
          bind:value={query}
          placeholder="Search names or exact Title IDs"
          autocomplete="off"
          spellcheck="false"
        />
      </div>
    </div>

    {#if refreshing}
      <div class="refresh-status" role="status" aria-live="polite">
        <div class="md-progress" role="progressbar" aria-label="Refreshing game library"></div>
        <span>Refreshing installed games and package matches…</span>
      </div>
    {:else if refreshPhase === 'error'}
      <div class="refresh-error" role="alert">
        <Icon name="warning" size={20} />
        <span>{refreshError || 'The game library could not be refreshed. Showing the last available games.'}</span>
      </div>
    {/if}

    {#if games.length === 0 && !refreshing}
      <div class="empty-state" role="status">
        <span class="empty-icon" aria-hidden="true"><Icon name="game" size={32} /></span>
        <h2>No games found</h2>
        <p>Refresh after adding games to Eden. Package setup is not required to list installed games.</p>
        <button type="button" class="md-button md-button--tonal" onclick={onrefresh}>
          <Icon name="refresh" size={18} />
          Refresh library
        </button>
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
          {@const entries = entriesForGroup(group).filter(item => item.entry.category !== 'base')}
          <article class="game-card md-card md-card--outlined" role="listitem">
            <button type="button" class="game-card__header md-list-item"
              disabled={!group.baseGame} data-title-id={group.baseGame?.titleId}
              aria-current={selectedTitleId === group.baseGame?.titleId ? 'true' : undefined}
              onclick={() => { if (group.baseGame) onselect(group.baseGame.titleId); }}>
              {#if group.baseImage}
                <img class="game-cover" src={group.baseImage} alt="" loading="lazy" decoding="async" />
              {:else}
                <span class="game-cover placeholder" aria-hidden="true">
                  <Icon name="game" size={28} />
                </span>
              {/if}

              <div class="game-heading">
                <h2>{group.baseName || group.baseTitleId}</h2>
                <code>{group.baseTitleId}</code>
                {#if group.baseInstalled}
                  <span class="installed-label"><Icon name="check" size={14} /> In Eden load</span>
                {/if}
              </div>
            </button>

            {#if entries.length > 0}
              <details class="related-titles">
                <summary>{group.updates.length} updates · {group.dlcs.length} DLC titles</summary>
                <div class="version-list">
                {#each entries as item (item.entry.titleId)}
                  <button
                    type="button"
                    class="version-entry md-list-item"
                    data-title-id={item.entry.titleId}
                    aria-current={selectedTitleId === item.entry.titleId ? 'true' : undefined}
                    onclick={() => onselect(item.entry.titleId)}
                  >
                    <span class="version-copy">
                      <span class="version-name">{item.entry.name || group.baseName || item.entry.titleId}</span>
                      <span class="version-meta">
                        <span class:unsupported={item.unsupported} class="version-kind">
                          {item.kind}{item.unsupported ? ' · Cheats unsupported' : ''}
                        </span>
                        <code>{item.entry.titleId}</code>
                      </span>
                    </span>
                    {#if item.entry.installed}
                      <span class="installed-label entry-installed">
                        <Icon name="check" size={14} /> In Eden load
                      </span>
                    {/if}
                  </button>
                {/each}
                </div>
              </details>
            {/if}
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
    border-bottom: 1px solid var(--md-sys-color-outline-variant);
    background: var(--md-sys-color-surface-container);
  }

  .title-block {
    display: flex;
    min-width: 0;
    align-items: center;
    gap: 0.75rem;
  }

  .brand {
    flex: 0 0 auto;
    color: var(--md-sys-color-primary);
    font-size: var(--md-sys-typescale-title-medium-size);
    font-weight: 700;
    letter-spacing: 0.12em;
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
    padding-inline-start: 3rem;
    border-radius: var(--md-sys-shape-corner-full);
    background: var(--md-sys-color-surface-container-high);
  }

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
    align-items: start;
    gap: 1rem;
    padding-block-end: 0.5rem;
  }

  .game-card {
    min-width: 0;
    overflow: hidden;
  }

  .game-card__header {
    display: flex;
    min-width: 0;
    align-items: center;
    gap: 1rem;
    padding: 1rem;
    border-bottom: 1px solid var(--md-sys-color-outline-variant);
    background: var(--md-sys-color-surface-container-low);
  }

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
  .version-name,
  .empty-state p {
    overflow-wrap: anywhere;
  }

  .game-heading h2 {
    color: var(--md-sys-color-on-surface);
    font-size: var(--md-sys-typescale-title-medium-size);
    font-weight: 500;
    line-height: 1.35rem;
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
    padding-inline: 0.5rem;
    border-radius: var(--md-sys-shape-corner-full);
    color: var(--md-sys-color-on-tertiary-container);
    background: var(--md-sys-color-tertiary-container);
    font-size: var(--md-sys-typescale-label-small-size);
    font-weight: 500;
    line-height: 1rem;
    white-space: nowrap;
  }

  .version-list {
    display: grid;
    gap: 1px;
    background: var(--md-sys-color-outline-variant);
  }

  .version-entry {
    min-width: 0;
    border-radius: 0;
    background: var(--md-sys-color-surface);
  }

  .version-copy {
    display: grid;
    min-width: 0;
    flex: 1;
    gap: 0.25rem;
  }

  .version-name {
    color: var(--md-sys-color-on-surface);
    font-size: var(--md-sys-typescale-body-large-size);
    font-weight: 500;
    line-height: 1.35rem;
  }

  .version-meta {
    display: flex;
    min-width: 0;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 0.25rem 0.75rem;
  }

  .version-kind {
    color: var(--md-sys-color-on-surface-variant);
    font-size: var(--md-sys-typescale-label-medium-size);
    line-height: 1rem;
  }

  .version-kind.unsupported {
    color: var(--md-sys-color-error);
  }

  .entry-installed {
    flex: 0 0 auto;
  }

  .related-titles > summary { min-height:48px; padding:12px 16px; cursor:pointer; font-size:14px; color:var(--md-sys-color-on-surface-variant); }

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

  @container library (min-width: 840px) {
    .game-grid {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }

  @container library (max-width: 420px) {
    .brand {
      display: none;
    }

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

    .version-entry {
      align-items: flex-start;
      flex-wrap: wrap;
    }

    .entry-installed {
      margin-inline-start: auto;
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
      grid-template-columns: minmax(0, 1fr);
      gap: 0.75rem;
    }

    .game-card__header {
      padding: 0.75rem;
    }
  }
</style>
