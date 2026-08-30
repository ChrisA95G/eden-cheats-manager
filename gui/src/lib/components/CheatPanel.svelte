<script>
  import { invoke } from '@tauri-apps/api/core';
  import { open as openDialog } from '@tauri-apps/plugin-dialog';
  import { selectedGame } from '../stores/games.js';

  let { settings, platform = 'desktop', isMobile = false } = $props();

  // Available cheats loaded from local DB
  let gameInfo = $state(/** @type {any} */ (null));
  let cheatsLoading = $state(false);
  let cheatsError = $state('');

  // Which build-id accordion row is open (by cheat.id)
  let expandedId = $state(/** @type {string | null} */ (null));

  // Detected build IDs on the device / PC for the selected game
  let detectedBuildIds = $state(/** @type {string[]} */ ([]));
  let detectingBuildIds = $state(false);

  // Installed cheats on the device / PC
  let installedCheats = $state(/** @type {any[]} */ ([]));
  let installedLoadError = $state('');
  let installedSet = $derived(new Set(installedCheats.map(ic => `${ic.buildId}_${ic.cheatName}`)));
  let installedLoading = $state(false);

  // Install / delete state — keyed by `${cheat.id}_${sectionName}`
  let installing = $state(/** @type {Record<string, boolean>} */ ({}));
  let deleting = $state(/** @type {Record<string, boolean>} */ ({}));
  let installMsg = $state(/** @type {Record<string, string>} */ ({}));

  // Scan Build ID state
  let scanningBuildId = $state(false);
  let scanBuildIdError = $state('');
  let settingRomPath = $state(false);

  // Fetch from API state
  let fetchingOnline = $state(false);
  let fetchOnlineMsg = $state('');
  let fetchOnlineError = $state('');
  let clearingApi = $state(false);

  // On mobile the installed cheats section starts collapsed
  let installedExpanded = $state(false);
  let didAutoExpand = $state(false);

  // Custom cheat form
  let showCustomForm = $state(false);
  let customBuildId = $state('');
  let customContent = $state('');
  let savingCustom = $state(false);
  let customSaveError = $state('');
  let deletingCustom = $state(/** @type {Record<string, boolean>} */ ({}));

  // Group cheat entries by build ID — declared here so effects below can reference it.
  // parseSections is a hoisted function declaration, so the forward reference is fine.
  let groupedCheats = $derived.by(() => {
    if (!gameInfo?.cheats?.length) return [];
    const map = new Map();
    for (const cheat of gameInfo.cheats) {
      const key = cheat.buildId.toUpperCase();
      if (!map.has(key)) {
        map.set(key, { buildId: key, credits: '', sections: [], customIds: [], hasCustom: false });
      }
      const g = map.get(key);
      parseSections(cheat.content).forEach(s => g.sections.push(s));
      if (cheat.custom) {
        g.customIds.push(cheat.id);
        g.hasCustom = true;
      } else if (!g.credits && cheat.credits) {
        g.credits = cheat.credits;
      }
    }
    return [...map.values()];
  });

  // Reload everything when the selected game changes
  $effect(() => {
    if ($selectedGame) {
      loadInstalledCheats($selectedGame);
      gameInfo = null;
      cheatsError = '';
      expandedId = null;
      detectedBuildIds = [];
      installing = {};
      installMsg = {};
      scanBuildIdError = '';
      showCustomForm = false;
      customBuildId = '';
      customContent = '';
      customSaveError = '';
      fetchOnlineMsg = '';
      fetchOnlineError = '';
      clearingApi = false;
      installedExpanded = false;
      didAutoExpand = false;
      searchCheats();
      detectBuildIds($selectedGame);
    }
  });

  // Auto-expand the best accordion row once per game load.
  // Desktop: only when a detected build ID matches. Mobile: always expand first available.
  // didAutoExpand prevents re-expanding when the user manually closes the accordion.
  $effect(() => {
    if (groupedCheats.length > 0 && !didAutoExpand) {
      const detected = groupedCheats.find(g => detectedBuildIds.includes(g.buildId));
      if (detected) {
        expandedId = detected.buildId;
        didAutoExpand = true;
      } else if (isMobile) {
        expandedId = groupedCheats[0].buildId;
        didAutoExpand = true;
      }
    }
  });

  // Prefer showing the detected ID that has actual cheats in the DB; fall back to first.
  let headerBuildId = $derived(detectedBuildIds.length === 0
    ? null
    : (groupedCheats.find(g => detectedBuildIds.includes(g.buildId))
        ?.buildId ?? detectedBuildIds[0]));

  /** @param {any} game */
  async function detectBuildIds(game) {
    if (settings.targetMode === 'androidNative') {
      detectedBuildIds = [];
      detectingBuildIds = false;
      return;
    }

    detectingBuildIds = true;
    try {
      let result;
      if (settings.targetMode === 'android') {
        result = await invoke('detect_build_ids_android', {
          adbPath: settings.adbPath,
          titleId: game.titleId,
        });
      } else {
        result = await invoke('detect_build_ids_pc', {
          loadDir: settings.pcLoadDir,
          titleId: game.titleId,
        });
      }
      detectedBuildIds = (result ?? []).map(/** @param {string} id */ id => id.toUpperCase());
    } catch (e) {
      // Detection is best-effort; errors are non-fatal
      console.debug('[build_ids] detect failed:', e);
      detectedBuildIds = [];
    } finally {
      detectingBuildIds = false;
    }
  }

  async function scanBuildId() {
    if (!$selectedGame || settings.targetMode === 'androidNative') return;
    scanningBuildId = true;
    scanBuildIdError = '';
    try {
      let bid;
      if (settings.targetMode === 'android') {
        bid = await invoke('scan_build_id_android', {
          adbPath: settings.adbPath,
          titleId: $selectedGame.titleId,
          baseTitleId: $selectedGame.baseTitleId ?? $selectedGame.titleId,
          gameName: $selectedGame.name ?? '',
        });
      } else {
        bid = await invoke('scan_build_id_pc', {
          loadDir: settings.pcLoadDir,
          titleId: $selectedGame.titleId,
          baseTitleId: $selectedGame.baseTitleId ?? $selectedGame.titleId,
          gameName: $selectedGame.name ?? '',
          edenExePath: settings.edenExePath ?? '',
        });
      }
      const upper = bid.toUpperCase();
      if (!detectedBuildIds.includes(upper)) {
        detectedBuildIds = [...detectedBuildIds, upper];
      }
    } catch (e) {
      scanBuildIdError = String(e);
    } finally {
      scanningBuildId = false;
    }
  }

  async function setRomPath() {
    if (!$selectedGame) return;
    settingRomPath = true;
    try {
      let defaultPath;
      try {
        const dirs = /** @type {string[]} */ (await invoke('get_eden_game_dirs_pc'));
        if (dirs.length > 0) defaultPath = dirs[0];
      } catch (_) {}
      const selected = await openDialog({
        title: `Select ROM for ${$selectedGame.name}`,
        filters: [{ name: 'Switch ROM', extensions: ['nsp', 'xci'] }],
        multiple: false,
        defaultPath,
      });
      if (selected) {
        const path = typeof selected === 'string' ? selected : selected.path;
        await invoke('set_rom_path_manual', {
          titleId: $selectedGame.baseTitleId ?? $selectedGame.titleId,
          path,
        });
      }
    } catch (e) {
      console.error('set ROM path failed:', e);
    } finally {
      settingRomPath = false;
    }
  }

  /** @param {any} game */
  async function loadInstalledCheats(game) {
    installedLoading = true;
    installedLoadError = '';
    try {
      let result;
      if (settings.targetMode === 'androidNative') {
        result = await invoke('list_installed_cheats_android_native', {
          titleId: game.titleId,
        });
      } else if (settings.targetMode === 'android') {
        result = await invoke('list_installed_cheats_android', { adbPath: settings.adbPath, titleId: game.titleId });
      } else {
        result = await invoke('list_installed_cheats_pc', { loadDir: settings.pcLoadDir, titleId: game.titleId });
      }
      installedCheats = result ?? [];
    } catch (e) {
      installedLoadError = String(e);
      installedCheats = [];
    } finally {
      installedLoading = false;
    }
  }

  async function searchCheats() {
    if (!$selectedGame) return;
    cheatsLoading = true;
    cheatsError = '';
    gameInfo = null;
    try {
      gameInfo = await invoke('search_cheats', { titleId: $selectedGame.titleId });
    } catch (e) {
      cheatsError = String(e);
    } finally {
      cheatsLoading = false;
    }
  }

  /**
   * Parse the raw cheat file content into individual named sections.
   * Each section starts with a [Name] header line.
   */
  /** @param {string} content */
  function parseSections(content) {
    const sections = [];
    let current = null;
    for (const line of content.split('\n')) {
      const t = line.trim();
      if (t.startsWith('[') && t.endsWith(']') && t.length > 2) {
        if (current) sections.push(current);
        current = { name: t.slice(1, -1), lines: [line] };
      } else if (current) {
        current.lines.push(line);
      }
    }
    if (current) sections.push(current);
    return sections.map(s => ({ name: s.name, content: s.lines.join('\n').trimEnd() }));
  }

  /** @param {string} sectionName */
  function toCheatName(sectionName) {
    return sectionName.slice(0, 60).replace(/[^\w\s\-()]/g, '').trim();
  }

  /**
   * @param {string} buildId
   * @param {any} section
   */
  async function installSection(buildId, section) {
    if (!$selectedGame) return;
    const key = `${buildId}_${section.name}`;
    installing[key] = true;
    installMsg[key] = '';
    try {
      const cheatName = toCheatName(section.name) || `cheat_${buildId}`;
      if (settings.targetMode === 'androidNative') {
        await invoke('install_cheat_android_native', {
          titleId: $selectedGame.titleId,
          cheatName,
          buildId,
          content: section.content,
        });
      } else if (settings.targetMode === 'android') {
        await invoke('install_cheat_android', {
          adbPath: settings.adbPath,
          titleId: $selectedGame.titleId,
          cheatName,
          buildId,
          content: section.content,
        });
      } else {
        await invoke('install_cheat_pc', {
          loadDir: settings.pcLoadDir,
          titleId: $selectedGame.titleId,
          cheatName,
          buildId,
          content: section.content,
        });
      }
      installMsg[key] = '✓';
      await loadInstalledCheats($selectedGame);
    } catch (e) {
      installMsg[key] = '✗';
    } finally {
      installing[key] = false;
    }
  }

  async function fetchOnline() {
    if (!$selectedGame) return;
    fetchingOnline = true;
    fetchOnlineMsg = '';
    fetchOnlineError = '';
    try {
      const added = await invoke('fetch_cheats_online', {
        titleId: $selectedGame.titleId,
        apiToken: settings.apiToken ?? '',
      });
      fetchOnlineMsg = added === 0
        ? 'Already up to date — no new cheats found.'
        : `${added} new cheat${added !== 1 ? 's' : ''} cached from Cheatslips.`;
      await searchCheats();
    } catch (e) {
      fetchOnlineError = String(e);
    } finally {
      fetchingOnline = false;
    }
  }

  async function clearApiCheats() {
    if (!$selectedGame) return;
    clearingApi = true;
    fetchOnlineMsg = '';
    fetchOnlineError = '';
    try {
      const deleted = await invoke('clear_api_cheats', { titleId: $selectedGame.titleId });
      fetchOnlineMsg = deleted === 0
        ? 'No API-fetched cheats to clear.'
        : `Cleared ${deleted} API-fetched cheat${deleted !== 1 ? 's' : ''} from local DB.`;
      await searchCheats();
    } catch (e) {
      fetchOnlineError = String(e);
    } finally {
      clearingApi = false;
    }
  }

  function openCustomForm() {
    customBuildId = headerBuildId ?? '';
    customContent = '';
    customSaveError = '';
    showCustomForm = true;
  }

  async function saveCustomCheat() {
    if (!$selectedGame) return;
    savingCustom = true;
    customSaveError = '';
    try {
      await invoke('save_custom_cheat', {
        titleId: $selectedGame.titleId,
        buildId: customBuildId,
        content: customContent,
      });
      showCustomForm = false;
      customBuildId = '';
      customContent = '';
      await searchCheats();
    } catch (e) {
      customSaveError = String(e);
    } finally {
      savingCustom = false;
    }
  }

  /** @param {any} group */
  async function deleteCustomCheat(group) {
    deletingCustom[group.buildId] = true;
    try {
      await Promise.all(group.customIds.map(/** @param {any} id */ id =>
        invoke('delete_custom_cheat', { cheatId: id })
      ));
      await searchCheats();
    } catch (e) {
      console.error('[custom_cheat] delete failed:', e);
    } finally {
      deletingCustom[group.buildId] = false;
    }
  }

  /** @param {any} ic */
  async function deleteInstalledCheat(ic) {
    if (!$selectedGame) return;
    const key = `del_${ic.cheatName}__${ic.buildId}`;
    deleting[key] = true;
    try {
      if (settings.targetMode === 'androidNative') {
        await invoke('delete_cheat_android_native', {
          titleId: $selectedGame.titleId,
          cheatName: ic.cheatName,
          buildId: ic.buildId,
        });
      } else if (settings.targetMode === 'android') {
        await invoke('delete_cheat_android', {
          adbPath: settings.adbPath,
          titleId: $selectedGame.titleId,
          cheatName: ic.cheatName,
          buildId: ic.buildId,
        });
      } else {
        await invoke('delete_cheat_pc', {
          loadDir: settings.pcLoadDir,
          titleId: $selectedGame.titleId,
          cheatName: ic.cheatName,
          buildId: ic.buildId,
        });
      }
      await loadInstalledCheats($selectedGame);
    } catch (_) {
    } finally {
      deleting[key] = false;
    }
  }

</script>

<main class="cheat-panel" class:mobile={isMobile}>
  {#if isMobile}
    <div class="back-bar">
      <button class="btn-back" onclick={() => selectedGame.set(null)}>← GAMES</button>
    </div>
  {/if}

  {#if !$selectedGame}
    <div class="empty-state">
      <div class="empty-prompt">&gt;_</div>
      <h2>NO GAME SELECTED</h2>
      <p>Scan your library from the sidebar, then select a game to manage cheats.</p>
    </div>

  {:else if $selectedGame.category === 'dlc'}
    <div class="game-header">
      <div class="game-header-overlay">
        {#if $selectedGame.image}
          <img src={$selectedGame.image} alt={$selectedGame.name} class="game-art" />
        {/if}
        <div class="game-title-info">
          <h1>{$selectedGame.name || $selectedGame.titleId}</h1>
          <code>{$selectedGame.titleId}</code>
        </div>
      </div>
    </div>
    <div class="dlc-info">
      <p>This is a DLC or add-on content. Cheats are not available for DLCs — only base games and updates support cheat management.</p>
    </div>

  {:else}
    <!-- Game header -->
    <div class="game-header">
      <div class="game-header-overlay">
        {#if gameInfo?.image || $selectedGame.image}
          <img src={gameInfo?.image ?? $selectedGame.image} alt={$selectedGame.name} class="game-art" />
        {/if}
        <div class="game-title-info">
          <h1>{$selectedGame.name || $selectedGame.titleId}</h1>
          <code>{$selectedGame.titleId}</code>
          {#if detectingBuildIds}
            <span class="header-build-id detecting">Detecting build…</span>
          {:else if headerBuildId}
            <span class="header-build-id">
              Build&nbsp;ID&nbsp;<code>{headerBuildId}</code>
              {#if detectedBuildIds.length > 1}
                <span class="header-build-extra">+{detectedBuildIds.length - 1} more</span>
              {/if}
            </span>
          {/if}
        </div>
      </div>
    </div>

    {#if settings.targetMode === 'androidNative'}
      <div class="native-build-notice">
        <strong>BUILD ID DETECTION UNAVAILABLE</strong>
        <span>Package-based detection is coming next. Until then, choose the matching Build ID row manually and verify it against your game version.</span>
      </div>
    {:else}
      <div class="scan-build-bar">
        <button
          class="btn-scan-build"
          disabled={scanningBuildId || detectingBuildIds}
          onclick={scanBuildId}
        >
          {scanningBuildId ? '[ SCANNING... ]' : '[ SCAN BUILD ID ]'}
        </button>
        {#if settings.targetMode === 'pc'}
          <button
            class="btn-set-rom"
            disabled={settingRomPath}
            onclick={setRomPath}
            title="Manually set the ROM file path for this game"
          >
            {settingRomPath ? '...' : '[ SET ROM PATH ]'}
          </button>
        {/if}
        {#if scanBuildIdError}
          <span class="scan-build-error">{scanBuildIdError}</span>
        {/if}
      </div>
      {#if settings.targetMode === 'android'}
        <p class="scan-build-hint">Device screen must be unlocked and on for scanning to work.</p>
      {:else}
        <p class="scan-build-hint">Eden launches automatically, reads the Build ID, then closes. Set the Eden executable path in Settings if not detected. Use [ SET ROM PATH ] if the game ROM is not auto-detected.</p>
      {/if}
    {/if}

    {#if cheatsError}
      <div class="error-msg">{cheatsError}</div>
    {/if}

    {#if installedLoadError}
      <div class="error-msg">⚠ Could not load installed cheats: {installedLoadError}</div>
    {/if}

    <!-- Detected-build hint: shown when detected build ID has no matching cheats in the DB -->
    {#if detectedBuildIds.length > 0 && groupedCheats.length > 0
        && !groupedCheats.some(g => detectedBuildIds.includes(g.buildId))}
      <div class="detected-hint">No cheats available for your version yet</div>
    {/if}

    <!-- Installed cheats -->
    {#if installedCheats.length > 0}
      <section class="section">
        <h3 class="section-title">
          {#if isMobile}
            <button class="btn-installed-toggle" onclick={() => installedExpanded = !installedExpanded}>
              {installedExpanded ? '▾' : '▸'} Installed Cheats
              <span class="count-badge">{installedCheats.length}</span>
            </button>
          {:else}
            Installed Cheats
          {/if}
        </h3>
        {#if !isMobile || installedExpanded}
          <div class="installed-list">
            {#each installedCheats as ic}
              {@const key = `del_${ic.cheatName}__${ic.buildId}`}
              <div class="installed-item">
                <div class="installed-info">
                  <span class="installed-name">{ic.cheatName}</span>
                  <code class="installed-bid">{ic.buildId}</code>
                </div>
                <button class="btn-delete" disabled={deleting[key]} onclick={() => deleteInstalledCheat(ic)}>
                  {deleting[key] ? '...' : '[ DEL ]'}
                </button>
              </div>
            {/each}
          </div>
        {/if}
      </section>
    {/if}

    <!-- Available cheats — accordion per build ID -->
    <section class="section">
      <h3 class="section-title">
        Available Cheats
        {#if groupedCheats.length > 0}
          <span class="count-badge">
            {groupedCheats.length} build{groupedCheats.length !== 1 ? 's' : ''}
          </span>
        {/if}
        <button
          class="btn-fetch-online"
          onclick={fetchOnline}
          disabled={fetchingOnline || clearingApi || !settings.apiToken}
          title={settings.apiToken ? 'Fetch cheats from Cheatslips API (3 requests/day limit)' : 'Set a Cheatslips API token in Settings to use this'}
        >{fetchingOnline ? '...' : '↓ API'}</button>
        <button
          class="btn-clear-api"
          onclick={clearApiCheats}
          disabled={clearingApi || fetchingOnline}
          title="Remove all API-fetched cheats for this game from the local DB"
        >{clearingApi ? '...' : '✕ API'}</button>
        <button class="btn-add-custom" onclick={openCustomForm} disabled={showCustomForm} title="Add custom cheat">+ Custom</button>
        <button class="btn-refresh" onclick={searchCheats} disabled={cheatsLoading} title="Refresh">↺</button>
      </h3>

      {#if fetchOnlineMsg}
        <div class="fetch-result ok">{fetchOnlineMsg}</div>
      {:else if fetchOnlineError}
        <div class="fetch-result err">{fetchOnlineError}</div>
      {/if}

      {#if showCustomForm}
        <div class="custom-form">
          <div class="custom-form-row">
            <label class="custom-label" for="custom-build-id">Build ID</label>
            <input
              id="custom-build-id"
              class="custom-input"
              type="text"
              placeholder="e.g. A1B2C3D4E5F60718"
              maxlength="16"
              bind:value={customBuildId}
            />
          </div>
          <div class="custom-form-row">
            <label class="custom-label" for="custom-content">Cheat content</label>
            <textarea
              id="custom-content"
              class="custom-textarea"
              placeholder="[Cheat Name]&#10;580F0000 00000000&#10;..."
              rows="6"
              bind:value={customContent}
            ></textarea>
          </div>
          {#if customSaveError}
            <div class="custom-error">{customSaveError}</div>
          {/if}
          <div class="custom-form-actions">
            <button class="btn-cancel" onclick={() => showCustomForm = false}>Cancel</button>
            <button class="btn-save-custom" onclick={saveCustomCheat} disabled={savingCustom}>
              {savingCustom ? 'Saving…' : 'Save cheat'}
            </button>
          </div>
        </div>
      {/if}

      {#if cheatsLoading}
        <div class="loading-msg">Loading cheats…</div>
      {:else if groupedCheats.length > 0}
        <div class="build-list">
          {#each groupedCheats as group}
            {@const isOpen = expandedId === group.buildId}
            {@const uninstalledSections = group.sections.filter(/** @param {any} s */ s => !installedSet.has(`${group.buildId}_${toCheatName(s.name)}`))}
            {@const installedCount = group.sections.length - uninstalledSections.length}
            <div class="build-group" class:open={isOpen}>

              <!-- Accordion header -->
              <div class="build-header-row">
                <button class="build-header" onclick={() => expandedId = isOpen ? null : group.buildId}>
                  <span class="build-arrow">{isOpen ? '▾' : '▸'}</span>
                  <span class="build-id">{group.buildId}</span>
                  <span class="build-meta">
                    {uninstalledSections.length} cheat{uninstalledSections.length !== 1 ? 's' : ''}
                    {#if group.credits}&nbsp;· <span class="credits-text">{group.credits}</span>{/if}
                    {#if installedCount > 0}
                      <span class="installed-count">{installedCount} installed</span>
                    {/if}
                  </span>
                  {#if detectedBuildIds.includes(group.buildId)}
                    <span class="detected-badge">✓ Detected</span>
                  {/if}
                  {#if group.hasCustom}
                    <span class="custom-badge">Custom</span>
                  {/if}
                </button>
                {#if group.hasCustom}
                  <button
                    class="btn-delete-custom"
                    title="Delete custom cheats from database"
                    disabled={deletingCustom[group.buildId]}
                    onclick={() => deleteCustomCheat(group)}
                  >{deletingCustom[group.buildId] ? '...' : '[ DEL ]'}</button>
                {/if}
              </div>

              <!-- Expanded section list -->
              {#if isOpen}
                <div class="section-list">
                  {#each uninstalledSections as section}
                    {@const skey = `${group.buildId}_${section.name}`}
                    <div class="section-item">
                      <span class="section-name">{section.name}</span>
                      <div class="section-actions">
                        {#if installMsg[skey]}
                          <span class="install-msg" class:ok={installMsg[skey] === '✓'}>
                            {installMsg[skey]}
                          </span>
                        {/if}
                        <button
                          class="btn-install"
                          disabled={installing[skey]}
                          onclick={() => installSection(group.buildId, section)}
                        >
                          {installing[skey] ? '...' : '[ INSTALL ]'}
                        </button>
                      </div>
                    </div>
                  {/each}
                </div>
              {/if}

            </div>
          {/each}
        </div>
      {:else if gameInfo}
        <div class="no-cheats">No cheats found locally for this game.</div>
      {/if}
    </section>
  {/if}
</main>

<style>
  .cheat-panel {
    flex: 1;
    display: flex;
    flex-direction: column;
    overflow-y: auto;
    background: var(--bg);
  }

  /* ── Empty / DLC states ── */
  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100%;
    color: var(--text-muted);
    gap: .75rem;
    padding: 2rem;
  }
  .empty-prompt {
    font-size: 2rem;
    color: var(--accent);
    letter-spacing: .1em;
    animation: blink-cursor 1.2s step-end infinite;
  }
  @keyframes blink-cursor {
    0%, 100% { opacity: 1; }
    50%       { opacity: 0.2; }
  }
  .empty-state h2 { margin: 0; color: var(--text); font-size: 1rem; letter-spacing: .15em; }
  .empty-state p  { font-size: .82rem; text-align: center; color: var(--text-muted); }
  .dlc-info { padding: 1.5rem; color: var(--text-muted); font-size: .82rem; line-height: 1.6; }

  /* ── Game header ── */
  .game-header {
    min-height: 100px;
    background: var(--surface);
    background-size: cover;
    background-position: center;
    flex-shrink: 0;
    border-bottom: 1px solid var(--border);
  }
  .game-header-overlay {
    min-height: 100px;
    background: linear-gradient(to right, rgba(8,6,0,.95) 0%, rgba(8,6,0,.7) 100%);
    display: flex;
    align-items: center;
    gap: 1rem;
    padding: 1rem 1.25rem;
  }
  .game-art {
    width: 64px;
    height: 64px;
    object-fit: cover;
    flex-shrink: 0;
    border: 1px solid var(--border);
    box-shadow: 0 0 12px rgba(0,0,0,.6);
  }
  .game-title-info { display: flex; flex-direction: column; gap: .2rem; }
  .game-title-info h1 { margin: 0; font-size: 1.1rem; letter-spacing: .04em; }
  .game-title-info > code { font-size: .72rem; color: var(--text-muted); letter-spacing: .06em; }

  .header-build-id {
    display: flex;
    align-items: center;
    gap: .4rem;
    font-size: .68rem;
    color: var(--accent);
    margin-top: .1rem;
    letter-spacing: .05em;
  }
  .header-build-id code {
    font-size: .72rem;
    color: var(--accent);
    letter-spacing: .06em;
  }
  .header-build-id.detecting {
    color: var(--text-muted);
    font-style: italic;
  }
  .header-build-extra {
    font-size: .65rem;
    color: var(--text-muted);
  }

  /* ── Misc ── */
  .error-msg {
    margin: .75rem 1.25rem 0;
    background: rgba(239,68,68,.1);
    color: var(--error);
    border-left: 2px solid var(--error);
    padding: .4rem .75rem;
    font-size: .78rem;
  }
  .no-cheats  { margin: .5rem 0 1.25rem; color: var(--text-muted); font-size: .82rem; }
  .loading-msg { color: var(--text-muted); font-size: .78rem; padding: .5rem 0 1.25rem; letter-spacing: .08em; }

  /* ── Sections ── */
  .section { padding: 1rem 1.25rem 0; }
  .section-title {
    margin: 0 0 .65rem;
    font-size: .72rem;
    letter-spacing: .12em;
    text-transform: uppercase;
    display: flex;
    align-items: center;
    gap: .5rem;
    color: var(--text-muted);
  }
  .section-title::before {
    content: '//';
    color: var(--text-dim);
    font-size: .65rem;
  }
  .count-badge {
    font-size: .68rem;
    background: var(--surface2);
    padding: .05rem .4rem;
    color: var(--text-muted);
    border: 1px solid var(--border);
    letter-spacing: .04em;
  }
  .btn-refresh {
    background: none;
    border: 1px solid var(--border);
    color: var(--text-muted);
    font-size: .75rem;
    padding: .1rem .4rem;
    cursor: pointer;
    font-family: inherit;
    transition: color .1s, border-color .1s;
  }
  .btn-refresh:not(:disabled):hover { color: var(--accent); border-color: var(--accent); }
  .btn-refresh:disabled { opacity: .35; cursor: default; }

  .btn-fetch-online {
    background: none;
    border: 1px solid var(--border);
    color: var(--text-muted);
    font-size: .68rem;
    padding: .1rem .45rem;
    cursor: pointer;
    font-family: inherit;
    letter-spacing: .05em;
    transition: color .1s, border-color .1s;
  }
  .btn-fetch-online:not(:disabled):hover { border-color: var(--accent); color: var(--accent); }
  .btn-fetch-online:disabled { opacity: .35; cursor: default; }

  .btn-clear-api {
    background: none;
    border: 1px solid var(--border);
    color: var(--text-muted);
    font-size: .68rem;
    padding: .1rem .45rem;
    cursor: pointer;
    font-family: inherit;
    letter-spacing: .05em;
    transition: color .1s, border-color .1s;
  }
  .btn-clear-api:not(:disabled):hover { border-color: var(--error); color: var(--error); }
  .btn-clear-api:disabled { opacity: .35; cursor: default; }

  .fetch-result {
    margin: 0 0 .5rem;
    padding: .3rem .6rem;
    font-size: .75rem;
    border-left: 2px solid transparent;
  }
  .fetch-result.ok  { background: rgba(74,222,128,.07); color: var(--success); border-left-color: var(--success); }
  .fetch-result.err { background: rgba(239,68,68,.07); color: var(--error); border-left-color: var(--error); }

  /* ── Installed cheats ── */
  .installed-list { display: flex; flex-direction: column; gap: .3rem; margin-bottom: 1rem; }
  .installed-item {
    display: flex;
    align-items: center;
    background: var(--surface);
    border: 1px solid var(--border);
    border-left: 2px solid var(--success);
    padding: .45rem .75rem;
    gap: .6rem;
  }
  .installed-info { display: flex; flex-direction: column; flex: 1; gap: .05rem; }
  .installed-name { font-size: .8rem; }
  .installed-bid  { font-size: .65rem; color: var(--text-muted); letter-spacing: .05em; }
  .btn-delete {
    background: none;
    border: 1px solid transparent;
    cursor: pointer;
    font-size: .65rem;
    padding: .1rem .35rem;
    opacity: .4;
    color: var(--text-muted);
    font-family: inherit;
    letter-spacing: .05em;
    transition: opacity .12s, color .12s, border-color .12s;
  }
  .btn-delete:not(:disabled):hover { opacity: 1; color: var(--error); border-color: var(--error); }

  /* ── Add-custom button ── */
  .btn-add-custom {
    background: none;
    border: 1px solid var(--border);
    color: var(--text-muted);
    font-size: .68rem;
    padding: .1rem .45rem;
    cursor: pointer;
    margin-left: auto;
    font-family: inherit;
    letter-spacing: .05em;
    transition: color .1s, border-color .1s;
  }
  .btn-add-custom:not(:disabled):hover { border-color: var(--accent); color: var(--accent); }
  .btn-add-custom:disabled { opacity: .35; cursor: default; }

  /* ── Custom cheat form ── */
  .custom-form {
    background: var(--surface);
    border: 1px solid var(--border);
    border-left: 2px solid var(--accent);
    padding: .85rem 1rem;
    margin-bottom: .65rem;
    display: flex;
    flex-direction: column;
    gap: .55rem;
  }
  .custom-form-row { display: flex; flex-direction: column; gap: .25rem; }
  .custom-label { font-size: .68rem; color: var(--text-muted); text-transform: uppercase; letter-spacing: .08em; }
  .custom-input {
    background: var(--bg);
    border: 1px solid var(--border);
    color: var(--text);
    font-family: inherit;
    font-size: .82rem;
    padding: .35rem .55rem;
    width: 100%;
    letter-spacing: .04em;
    transition: border-color .1s;
    outline: none;
  }
  .custom-input:focus { border-color: var(--accent); box-shadow: 0 0 0 1px var(--accent-glow); }
  .custom-textarea {
    background: var(--bg);
    border: 1px solid var(--border);
    color: var(--text);
    font-family: inherit;
    font-size: .78rem;
    padding: .35rem .55rem;
    width: 100%;
    resize: vertical;
    line-height: 1.5;
    transition: border-color .1s;
    outline: none;
  }
  .custom-textarea:focus { border-color: var(--accent); }
  .custom-error { font-size: .75rem; color: var(--error); }
  .custom-form-actions { display: flex; gap: .45rem; justify-content: flex-end; }
  .btn-cancel {
    background: none;
    border: 1px solid var(--border);
    color: var(--text-muted);
    font-size: .75rem;
    padding: .25rem .65rem;
    cursor: pointer;
    font-family: inherit;
    letter-spacing: .05em;
    transition: color .1s;
  }
  .btn-cancel:hover { color: var(--text); }
  .btn-save-custom {
    background: var(--accent-dim);
    color: var(--accent);
    border: 1px solid var(--accent);
    font-size: .75rem;
    padding: .25rem .8rem;
    cursor: pointer;
    font-family: inherit;
    letter-spacing: .08em;
    transition: background .15s, box-shadow .15s;
  }
  .btn-save-custom:not(:disabled):hover { background: var(--accent-dim); box-shadow: 0 0 6px var(--accent-glow); }
  .btn-save-custom:disabled { opacity: .35; cursor: default; }

  /* ── Build-ID accordion ── */
  .build-list { display: flex; flex-direction: column; gap: .35rem; padding-bottom: 1.5rem; }

  .build-group {
    background: var(--surface);
    border: 1px solid var(--border);
    overflow: hidden;
  }
  .build-group.open { border-color: var(--accent); box-shadow: 0 0 8px var(--accent-glow); }

  .build-header-row { display: flex; align-items: center; }
  .build-header {
    flex: 1;
    display: flex;
    align-items: center;
    gap: .55rem;
    padding: .6rem .85rem;
    background: none;
    border: none;
    color: var(--text);
    cursor: pointer;
    text-align: left;
    min-width: 0;
    transition: background .1s;
  }
  .build-header:hover { background: rgba(245, 168, 0, 0.04); }

  .btn-delete-custom {
    background: none;
    border: none;
    cursor: pointer;
    font-size: .85rem;
    padding: .6rem .75rem;
    opacity: .35;
    flex-shrink: 0;
    color: var(--text-muted);
    transition: opacity .1s, color .1s;
  }
  .btn-delete-custom:not(:disabled):hover { opacity: 1; color: var(--error); }
  .btn-delete-custom:disabled { opacity: .2; cursor: default; }

  .custom-badge {
    font-size: .62rem;
    background: rgba(245, 168, 0, 0.12);
    color: var(--accent);
    padding: .05rem .35rem;
    border: 1px solid rgba(245, 168, 0, 0.25);
    flex-shrink: 0;
    letter-spacing: .06em;
    text-transform: uppercase;
  }

  .build-arrow { font-size: .6rem; color: var(--text-dim); flex-shrink: 0; width: .7rem; }
  .build-id {
    font-size: .8rem;
    letter-spacing: .05em;
    flex-shrink: 0;
  }
  .build-meta {
    font-size: .72rem;
    color: var(--text-muted);
    display: flex;
    align-items: center;
    gap: .35rem;
    flex: 1;
    flex-wrap: wrap;
  }
  .installed-count {
    background: rgba(200, 134, 12, 0.1);
    color: var(--success);
    padding: .02rem .3rem;
    font-size: .65rem;
    border: 1px solid rgba(200, 134, 12, 0.25);
    letter-spacing: .04em;
  }

  .credits-text {
    display: inline-block;
    max-width: 40ch;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    vertical-align: bottom;
  }

  /* ── Cheat section list ── */
  .section-list {
    border-top: 1px solid var(--border);
    display: flex;
    flex-direction: column;
  }
  .section-item {
    display: flex;
    align-items: center;
    padding: .45rem .85rem .45rem 1.75rem;
    gap: .65rem;
    border-bottom: 1px solid rgba(53, 37, 16, 0.5);
    transition: background .1s;
  }
  .section-item:last-child { border-bottom: none; }
  .section-item:hover { background: rgba(245, 168, 0, 0.03); }

  .section-name { flex: 1; font-size: .82rem; }
  .section-actions { display: flex; align-items: center; gap: .45rem; flex-shrink: 0; }

  .btn-install {
    background: transparent;
    color: var(--accent);
    border: 1px solid var(--accent);
    padding: .2rem .6rem;
    font-size: .72rem;
    cursor: pointer;
    font-family: inherit;
    letter-spacing: .08em;
    transition: background .15s, box-shadow .15s;
    white-space: nowrap;
  }
  .btn-install:not(:disabled):hover {
    background: var(--accent-dim);
    box-shadow: 0 0 5px var(--accent-glow);
  }
  .btn-install:disabled { opacity: .35; cursor: default; }

  .install-msg { font-size: .72rem; color: var(--text-muted); }
  .install-msg.ok { color: var(--success); }

  /* ── Detection ── */
  .detected-badge {
    font-size: .62rem;
    background: rgba(245, 168, 0, 0.12);
    color: var(--accent);
    padding: .03rem .35rem;
    border: 1px solid rgba(245, 168, 0, 0.25);
    flex-shrink: 0;
    letter-spacing: .05em;
    text-transform: uppercase;
  }
  .detected-hint {
    margin: .5rem 1.25rem 0;
    background: var(--surface);
    border-left: 2px solid var(--text-dim);
    color: var(--text-muted);
    padding: .4rem .75rem;
    font-size: .76rem;
  }

  /* ── Scan Build ID ── */
  .native-build-notice {
    display: grid;
    gap: .25rem;
    margin: .6rem 1.25rem .35rem;
    padding: .65rem .75rem;
    border-left: 2px solid var(--accent);
    background: var(--surface);
    color: var(--text-muted);
    font-size: .72rem;
    line-height: 1.5;
  }
  .native-build-notice strong { color: var(--accent); letter-spacing: .08em; }
  .scan-build-bar {
    display: flex;
    align-items: center;
    gap: .55rem;
    padding: .5rem 1.25rem;
    flex-shrink: 0;
  }
  .btn-scan-build {
    background: transparent;
    border: 1px solid var(--border);
    color: var(--text-muted);
    font-size: .75rem;
    padding: .25rem .75rem;
    cursor: pointer;
    font-family: inherit;
    letter-spacing: .08em;
    transition: color .12s, border-color .12s, box-shadow .12s;
    white-space: nowrap;
  }
  .btn-scan-build:not(:disabled):hover {
    border-color: var(--accent);
    color: var(--accent);
    box-shadow: 0 0 5px var(--accent-glow);
  }
  .btn-scan-build:disabled { opacity: .4; cursor: default; }
  .btn-set-rom {
    background: transparent;
    border: 1px solid var(--border);
    color: var(--text-muted);
    font-size: .75rem;
    padding: .25rem .75rem;
    cursor: pointer;
    font-family: inherit;
    letter-spacing: .08em;
    transition: color .12s, border-color .12s;
    white-space: nowrap;
  }
  .btn-set-rom:not(:disabled):hover {
    border-color: var(--text-dim);
    color: var(--text-dim);
  }
  .btn-set-rom:disabled { opacity: .4; cursor: default; }
  .scan-build-hint {
    font-size: .7rem;
    color: var(--text-dim);
    margin: -.15rem 1.25rem .35rem;
  }
  .scan-build-error { font-size: .76rem; color: var(--error); flex: 1; }

  .btn-installed-toggle {
    background: none;
    border: none;
    color: inherit;
    font: inherit;
    font-size: .72rem;
    letter-spacing: .12em;
    text-transform: uppercase;
    cursor: pointer;
    padding: 0;
    display: flex;
    align-items: center;
    gap: .4rem;
  }

  /* ── Mobile overrides ── */
  .back-bar {
    padding: .4rem .75rem;
    border-bottom: 1px solid var(--border);
    background: var(--surface);
    flex-shrink: 0;
  }
  .btn-back {
    background: none;
    border: none;
    color: var(--text-muted);
    font-family: inherit;
    font-size: .78rem;
    letter-spacing: .1em;
    cursor: pointer;
    padding: .3rem .5rem;
    transition: color .12s;
  }
  .btn-back:hover { color: var(--accent); }

  .cheat-panel.mobile .section-item {
    min-height: 48px;
    padding-top: .6rem;
    padding-bottom: .6rem;
  }
  .cheat-panel.mobile .btn-install {
    padding: .35rem .75rem;
    font-size: .78rem;
  }
  .cheat-panel.mobile .btn-delete {
    padding: .25rem .55rem;
    font-size: .72rem;
    opacity: .7;
  }
  .cheat-panel.mobile .scan-build-bar {
    flex-wrap: wrap;
    gap: .4rem;
    padding: .65rem 1rem;
  }
  .cheat-panel.mobile .build-header {
    padding: .75rem .85rem;
  }
  .cheat-panel.mobile {
    height: 100dvh;
    overflow-y: auto;
    padding-bottom: env(safe-area-inset-bottom);
  }

  /* Push back bar below status bar / punch-hole camera */
  .cheat-panel.mobile .back-bar {
    padding-top: max(.65rem, calc(env(safe-area-inset-top) + .4rem));
    padding-left: max(.75rem, calc(env(safe-area-inset-left) + .5rem));
    padding-right: max(.75rem, calc(env(safe-area-inset-right) + .5rem));
    min-height: 60px;
  }
  .cheat-panel.mobile .btn-back {
    font-size: .88rem;
    min-height: 44px;
    padding: .5rem .6rem;
  }

  .cheat-panel.mobile .game-header,
  .cheat-panel.mobile .game-header-overlay {
    min-height: 70px;
  }
  .cheat-panel.mobile .game-header-overlay {
    padding: .65rem 1rem;
    gap: .75rem;
  }
  .cheat-panel.mobile .game-art {
    width: 48px;
    height: 48px;
  }
  .cheat-panel.mobile .game-title-info h1 {
    font-size: .95rem;
  }
  .cheat-panel.mobile .section-title {
    flex-wrap: wrap;
    gap: .35rem;
  }
</style>
