<script>
  import { onMount } from 'svelte';
  import { games, gamesLoading, gamesError, scanGames, loadCachedGamesThenRescan, selectedGame } from '../stores/games.js';

  /** @type {{ settings: any, platform: 'android' | 'desktop', isMobile?: boolean, onopenSettings: function }} */
  let { settings, platform, isMobile = false, onopenSettings } = $props();

  let expandedGroups = $state(/** @type {Set<string>} */ (new Set()));

  let modeBadge = $derived(platform === 'android' ? 'ANDROID' : 'PC');
  let deviceStatusLabel = $derived(
    platform === 'android' ? 'On-device' : (settings?.pcLoadDir || 'No load dir set')
  );

  onMount(async () => {
    // Show cached games instantly on both platforms, then rescan for changes.
    loadCachedGamesThenRescan(settings, platform);
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

</script>

<aside class="sidebar" class:mobile={isMobile}>
  <div class="sidebar-header">
    <span class="app-brand">ECM</span>
    <span class="brand-sep">//</span>
    <span class="mode-badge">{modeBadge}</span>
    <div class="status-group">
      <div class="status-dot ok"></div>
      <span class="status-label">{deviceStatusLabel}</span>
    </div>
    <button class="btn-icon" title="Settings" onclick={() => onopenSettings?.()}>SYS</button>
  </div>

  <button
    class="scan-btn"
    disabled={$gamesLoading}
    onclick={() => scanGames(settings, platform)}
  >
    {$gamesLoading ? '[ SCANNING... ]' : '[ SCAN LIBRARY ]'}
  </button>

  {#if $gamesError}
    <div class="error-msg">{$gamesError}</div>
  {/if}

  <div class="game-list">
    {#if $games.length === 0 && !$gamesLoading}
      <div class="empty-state">
        <p class="empty-prompt">&gt; NO GAMES FOUND</p>
        <p class="hint">Run [ SCAN LIBRARY ] to load your Eden game library.</p>
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
    height: 100dvh;
    overflow: hidden;
  }

  /* Header */
  .sidebar-header {
    display: flex;
    align-items: center;
    gap: .5rem;
    padding: .65rem .85rem;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
    background: var(--surface2);
  }
  .app-brand {
    font-size: .9rem;
    letter-spacing: .25em;
    color: var(--accent);
    flex-shrink: 0;
  }
  .brand-sep {
    font-size: .72rem;
    color: var(--text-dim);
    flex-shrink: 0;
    letter-spacing: .05em;
  }
  .mode-badge {
    font-size: .62rem;
    color: var(--accent);
    border: 1px solid var(--accent);
    background: var(--accent-dim);
    padding: .05rem .35rem;
    letter-spacing: .1em;
    flex-shrink: 0;
  }
  .status-group {
    display: flex;
    align-items: center;
    gap: .35rem;
    flex: 1;
    overflow: hidden;
  }
  .status-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    flex-shrink: 0;
    border: 1px solid var(--text-muted);
  }
  .status-dot.ok   { background: var(--accent); border-color: var(--accent); box-shadow: 0 0 4px var(--accent-glow); }
  .status-label {
    flex: 1;
    font-size: .72rem;
    color: var(--text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    letter-spacing: .03em;
  }
  .btn-icon {
    background: none;
    border: 1px solid var(--border);
    color: var(--text-muted);
    cursor: pointer;
    font-size: .65rem;
    padding: .15rem .35rem;
    letter-spacing: .1em;
    flex-shrink: 0;
    transition: color .12s, border-color .12s;
  }
  .btn-icon:hover { color: var(--accent); border-color: var(--accent); }

  /* Scan button */
  .scan-btn {
    margin: .6rem .75rem;
    background: transparent;
    color: var(--accent);
    border: 1px solid var(--accent);
    padding: .45rem 1rem;
    font-size: .78rem;
    letter-spacing: .1em;
    cursor: pointer;
    transition: background .15s, box-shadow .15s;
    flex-shrink: 0;
    font-family: inherit;
  }
  .scan-btn:not(:disabled):hover {
    background: var(--accent-dim);
    box-shadow: 0 0 8px var(--accent-glow);
  }
  .scan-btn:disabled { opacity: .35; cursor: default; }

  .error-msg {
    margin: 0 .75rem .4rem;
    background: var(--surface2);
    color: var(--error);
    border-left: 2px solid var(--error);
    padding: .35rem .6rem;
    font-size: .72rem;
  }


  /* Game list */
  .game-list { flex: 1; overflow-y: auto; padding: .2rem .4rem; }
  .empty-state { padding: 2rem .75rem; color: var(--text-muted); }
  .empty-prompt { font-size: .82rem; color: var(--text-muted); margin-bottom: .5rem; letter-spacing: .03em; }
  .empty-state .hint { font-size: .72rem; opacity: .7; line-height: 1.5; }

  /* Game groups */
  .game-group { margin-bottom: 1px; }
  .group-header {
    width: 100%;
    display: flex;
    align-items: center;
    gap: .5rem;
    background: none;
    border: none;
    border-left: 2px solid transparent;
    padding: .4rem .5rem;
    cursor: pointer;
    text-align: left;
    color: var(--text);
    transition: background .1s, border-color .1s;
  }
  .group-header:hover { background: rgba(245, 168, 0, 0.04); }
  .group-header.active {
    background: var(--accent-dim);
    border-left-color: var(--accent);
    padding-left: calc(.5rem - 2px);
  }
  .expand-chevron { font-size: .6rem; width: 10px; flex-shrink: 0; color: var(--text-dim); }

  /* Group children */
  .group-children {
    padding-left: 1.2rem;
    border-left: 1px solid var(--border);
    margin-left: .85rem;
  }
  .section-label {
    font-size: .6rem;
    text-transform: uppercase;
    letter-spacing: .1em;
    color: var(--text-dim);
    padding: .3rem .4rem .1rem;
  }
  .child-item {
    width: 100%;
    display: flex;
    align-items: center;
    gap: .5rem;
    background: none;
    border: none;
    border-left: 2px solid transparent;
    padding: .3rem .5rem;
    cursor: pointer;
    text-align: left;
    color: var(--text);
    transition: background .1s, border-color .1s;
    opacity: .55;
  }
  .child-item:hover { background: rgba(245, 168, 0, 0.04); opacity: .8; }
  .child-item.active {
    background: var(--accent-dim);
    border-left-color: var(--accent);
    padding-left: calc(.5rem - 2px);
    opacity: 1;
  }

  /* Game art */
  .game-cover {
    width: 34px;
    height: 34px;
    object-fit: cover;
    flex-shrink: 0;
    border: 1px solid var(--border);
  }
  .game-cover-placeholder {
    width: 34px;
    height: 34px;
    background: var(--surface2);
    border: 1px solid var(--border);
    flex-shrink: 0;
  }
  .game-info { display: flex; flex-direction: column; gap: .05rem; overflow: hidden; flex: 1; }
  .game-name { font-size: .8rem; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .game-tid { font-size: .65rem; color: var(--text-muted); letter-spacing: .03em; }
  .installed-dot { color: var(--success); font-size: .55rem; flex-shrink: 0; line-height: 1; }

  /* ── Mobile overrides ── */
  .sidebar.mobile {
    width: 100%;
    max-width: 100%;
    min-width: unset;
    border-right: none;
  }
  /* Push header below punch-hole camera / status bar on edge-to-edge Android */
  .sidebar.mobile .sidebar-header {
    padding-top: max(1rem, calc(env(safe-area-inset-top) + .4rem));
    padding-left: max(.85rem, calc(env(safe-area-inset-left) + .5rem));
    padding-right: max(.85rem, calc(env(safe-area-inset-right) + .5rem));
    min-height: 60px;
    align-items: center;
  }
  .sidebar.mobile .app-brand { font-size: 1.05rem; }
  .sidebar.mobile .mode-badge { font-size: .72rem; padding: .1rem .45rem; }
  .sidebar.mobile .status-label { font-size: .8rem; }
  .sidebar.mobile .btn-icon {
    font-size: .78rem;
    padding: .35rem .6rem;
    min-height: 36px;
    min-width: 44px;
  }
  .sidebar.mobile .group-header,
  .sidebar.mobile .child-item {
    min-height: 52px;
    padding-top: .65rem;
    padding-bottom: .65rem;
  }
  .sidebar.mobile .scan-btn {
    padding: .75rem 1rem;
    font-size: .88rem;
  }
  .sidebar.mobile .game-cover,
  .sidebar.mobile .game-cover-placeholder {
    width: 44px;
    height: 44px;
  }
  .sidebar.mobile .game-name { font-size: .9rem; }
  .sidebar.mobile .game-tid  { font-size: .72rem; }
</style>
