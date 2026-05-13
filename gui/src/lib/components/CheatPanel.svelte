<script>
  import { invoke } from '@tauri-apps/api/core';
  import { selectedGame } from '../stores/games.js';

  let { settings } = $props();

  // Available cheats loaded from local DB
  let gameInfo = $state(null);
  let cheatsLoading = $state(false);
  let cheatsError = $state('');

  // Which build-id accordion row is open (by cheat.id)
  let expandedId = $state(null);

  // Detected build IDs on the device / PC for the selected game
  let detectedBuildIds = $state([]);
  let detectingBuildIds = $state(false);

  // Installed cheats on the device / PC
  let installedCheats = $state([]);
  let installedLoadError = $state('');
  let installedSet = $derived(new Set(installedCheats.map(ic => `${ic.buildId}_${ic.cheatName}`)));
  let installedLoading = $state(false);

  // Install / delete state — keyed by `${cheat.id}_${sectionName}`
  let installing = $state({});
  let deleting = $state({});
  let installMsg = $state({});

  // Scan Build ID state
  let scanningBuildId = $state(false);
  let scanBuildIdError = $state('');

  // Custom cheat form
  let showCustomForm = $state(false);
  let customBuildId = $state('');
  let customContent = $state('');
  let savingCustom = $state(false);
  let customSaveError = $state('');
  let deletingCustom = $state({});

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
      searchCheats();
      detectBuildIds($selectedGame);
    }
  });

  // Auto-expand the accordion row that matches a detected build ID.
  // Only fires once per game selection (when expandedId is still null).
  $effect(() => {
    if (groupedCheats.length > 0 && detectedBuildIds.length > 0 && expandedId === null) {
      const match = groupedCheats.find(g => detectedBuildIds.includes(g.buildId));
      if (match) expandedId = match.buildId;
    }
  });

  // Prefer showing the detected ID that has actual cheats in the DB; fall back to first.
  let headerBuildId = $derived(detectedBuildIds.length === 0
    ? null
    : (groupedCheats.find(g => detectedBuildIds.includes(g.buildId))
        ?.buildId ?? detectedBuildIds[0]));

  async function detectBuildIds(game) {
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
      detectedBuildIds = (result ?? []).map(id => id.toUpperCase());
    } catch (e) {
      // Detection is best-effort; errors are non-fatal
      console.debug('[build_ids] detect failed:', e);
      detectedBuildIds = [];
    } finally {
      detectingBuildIds = false;
    }
  }

  async function scanBuildId() {
    if (!$selectedGame) return;
    scanningBuildId = true;
    scanBuildIdError = '';
    try {
      const bid = await invoke('scan_build_id_android', {
        adbPath: settings.adbPath,
        titleId: $selectedGame.titleId,
      });
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

  async function loadInstalledCheats(game) {
    installedLoading = true;
    installedLoadError = '';
    try {
      let result;
      if (settings.targetMode === 'android') {
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

  /** Sanitise a section name for use as a directory / file label. */
  function toCheatName(sectionName) {
    return sectionName.slice(0, 60).replace(/[^\w\s\-()]/g, '').trim();
  }

  async function installSection(buildId, section) {
    const key = `${buildId}_${section.name}`;
    installing[key] = true;
    installMsg[key] = '';
    try {
      const cheatName = toCheatName(section.name) || `cheat_${buildId}`;
      if (settings.targetMode === 'android') {
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

  async function deleteCustomCheat(group) {
    deletingCustom[group.buildId] = true;
    try {
      await Promise.all(group.customIds.map(id =>
        invoke('delete_custom_cheat', { cheatId: id })
      ));
      await searchCheats();
    } catch (e) {
      console.error('[custom_cheat] delete failed:', e);
    } finally {
      deletingCustom[group.buildId] = false;
    }
  }

  async function deleteInstalledCheat(ic) {
    const key = `del_${ic.cheatName}__${ic.buildId}`;
    deleting[key] = true;
    try {
      if (settings.targetMode === 'android') {
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

<main class="cheat-panel">
  {#if !$selectedGame}
    <div class="empty-state">
      <div class="empty-icon">🎮</div>
      <h2>No game selected</h2>
      <p>Scan your library from the sidebar, then click a game to manage its cheats.</p>
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

    {#if settings.targetMode === 'android'}
      <div class="scan-build-bar">
        <button
          class="btn-scan-build"
          disabled={scanningBuildId || detectingBuildIds}
          onclick={scanBuildId}
        >
          {scanningBuildId ? 'Scanning…' : '⟳ Scan Build ID'}
        </button>
        {#if scanBuildIdError}
          <span class="scan-build-error">{scanBuildIdError}</span>
        {/if}
      </div>
      <p class="scan-build-hint">Device screen must be unlocked and on for scanning to work.</p>
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
        <h3 class="section-title">Installed Cheats</h3>
        <div class="installed-list">
          {#each installedCheats as ic}
            {@const key = `del_${ic.cheatName}__${ic.buildId}`}
            <div class="installed-item">
              <div class="installed-info">
                <span class="installed-name">{ic.cheatName}</span>
                <code class="installed-bid">{ic.buildId}</code>
              </div>
              <button class="btn-delete" disabled={deleting[key]} onclick={() => deleteInstalledCheat(ic)}>
                {deleting[key] ? '…' : '🗑'}
              </button>
            </div>
          {/each}
        </div>
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
        <button class="btn-add-custom" onclick={openCustomForm} disabled={showCustomForm} title="Add custom cheat">+ Custom</button>
        <button class="btn-refresh" onclick={searchCheats} disabled={cheatsLoading} title="Refresh">↺</button>
      </h3>

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
            {@const uninstalledSections = group.sections.filter(s => !installedSet.has(`${group.buildId}_${toCheatName(s.name)}`))}
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
                  >{deletingCustom[group.buildId] ? '…' : '🗑'}</button>
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
                          {installing[skey] ? '…' : '⬇ Install'}
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

  /* ── Empty / DLC states ─────────────────────────────────────── */
  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100%;
    color: var(--text-muted);
    gap: .5rem;
    padding: 2rem;
  }
  .empty-icon { font-size: 3.5rem; }
  .empty-state h2 { margin: 0; color: var(--text); font-size: 1.2rem; }
  .empty-state p  { font-size: .9rem; text-align: center; }
  .dlc-info { padding: 1.5rem; color: var(--text-muted); font-size: .88rem; line-height: 1.5; }

  /* ── Game header ────────────────────────────────────────────── */
  .game-header {
    min-height: 110px;
    background: var(--surface);
    background-size: cover;
    background-position: center;
    flex-shrink: 0;
  }
  .game-header-overlay {
    min-height: 110px;
    background: linear-gradient(to right, rgba(16,16,24,.92) 0%, rgba(16,16,24,.6) 100%);
    display: flex;
    align-items: center;
    gap: 1.25rem;
    padding: 1.25rem 1.5rem;
  }
  .game-art { width: 72px; height: 72px; border-radius: 8px; object-fit: cover; box-shadow: 0 4px 12px rgba(0,0,0,.5); flex-shrink: 0; }
  .game-title-info { display: flex; flex-direction: column; gap: .25rem; }
  .game-title-info h1 { margin: 0; font-size: 1.4rem; }
  .game-title-info > code { font-size: .8rem; color: var(--text-muted); }

  .header-build-id {
    display: flex;
    align-items: center;
    gap: .4rem;
    font-size: .75rem;
    color: #34c759;
    font-weight: 500;
    margin-top: .1rem;
  }
  .header-build-id code {
    font-size: .78rem;
    font-weight: 700;
    color: #34c759;
    letter-spacing: .04em;
  }
  .header-build-id.detecting {
    color: var(--text-muted);
    font-style: italic;
    font-weight: 400;
  }
  .header-build-extra {
    font-size: .7rem;
    color: var(--text-muted);
    font-weight: 400;
  }

  /* ── Misc ───────────────────────────────────────────────────── */
  .error-msg { margin: .75rem 1.5rem 0; background: rgba(255,59,48,.15); color: #ff3b30; border-radius: 8px; padding: .5rem .9rem; font-size: .85rem; }
  .no-cheats  { margin: .5rem 0 1.5rem; color: var(--text-muted); font-size: .9rem; }
  .loading-msg { color: var(--text-muted); font-size: .88rem; padding: .5rem 0 1.5rem; }

  /* ── Sections ───────────────────────────────────────────────── */
  .section { padding: 1.25rem 1.5rem 0; }
  .section-title {
    margin: 0 0 .75rem;
    font-size: .95rem;
    display: flex;
    align-items: center;
    gap: .5rem;
  }
  .count-badge {
    font-size: .75rem;
    font-weight: 400;
    background: var(--surface2);
    border-radius: 4px;
    padding: .1rem .4rem;
    color: var(--text-muted);
  }
  .btn-refresh {
    background: none;
    border: 1px solid var(--border);
    border-radius: 5px;
    color: var(--text-muted);
    font-size: .85rem;
    padding: .15rem .45rem;
    cursor: pointer;
  }
  .btn-refresh:not(:disabled):hover { color: var(--text); }
  .btn-refresh:disabled { opacity: .4; cursor: default; }

  /* ── Installed cheats ───────────────────────────────────────── */
  .installed-list { display: flex; flex-direction: column; gap: .4rem; margin-bottom: 1.25rem; }
  .installed-item { display: flex; align-items: center; background: var(--surface); border-radius: 8px; padding: .5rem .85rem; gap: .75rem; }
  .installed-info { display: flex; flex-direction: column; flex: 1; gap: .1rem; }
  .installed-name { font-size: .85rem; font-weight: 500; }
  .installed-bid  { font-size: .72rem; color: var(--text-muted); }
  .btn-delete { background: none; border: none; cursor: pointer; font-size: 1rem; padding: .2rem; opacity: .5; }
  .btn-delete:not(:disabled):hover { opacity: 1; }

  /* ── Add-custom button ──────────────────────────────────────── */
  .btn-add-custom {
    background: none;
    border: 1px solid var(--border);
    border-radius: 5px;
    color: var(--text-muted);
    font-size: .78rem;
    padding: .15rem .5rem;
    cursor: pointer;
    margin-left: auto;
  }
  .btn-add-custom:not(:disabled):hover { border-color: var(--accent); color: var(--accent); }
  .btn-add-custom:disabled { opacity: .4; cursor: default; }

  /* ── Custom cheat form ──────────────────────────────────────── */
  .custom-form {
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 1rem;
    margin-bottom: .75rem;
    display: flex;
    flex-direction: column;
    gap: .65rem;
  }
  .custom-form-row { display: flex; flex-direction: column; gap: .3rem; }
  .custom-label { font-size: .78rem; color: var(--text-muted); font-weight: 500; }
  .custom-input {
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 6px;
    color: var(--text);
    font-family: monospace;
    font-size: .88rem;
    padding: .4rem .65rem;
    width: 100%;
    box-sizing: border-box;
  }
  .custom-input:focus { outline: none; border-color: var(--accent); }
  .custom-textarea {
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 6px;
    color: var(--text);
    font-family: monospace;
    font-size: .82rem;
    padding: .4rem .65rem;
    width: 100%;
    box-sizing: border-box;
    resize: vertical;
    line-height: 1.5;
  }
  .custom-textarea:focus { outline: none; border-color: var(--accent); }
  .custom-error { font-size: .8rem; color: #ff3b30; }
  .custom-form-actions { display: flex; gap: .5rem; justify-content: flex-end; }
  .btn-cancel {
    background: none;
    border: 1px solid var(--border);
    border-radius: 6px;
    color: var(--text-muted);
    font-size: .82rem;
    padding: .3rem .75rem;
    cursor: pointer;
  }
  .btn-cancel:hover { color: var(--text); }
  .btn-save-custom {
    background: var(--accent);
    color: #fff;
    border: none;
    border-radius: 6px;
    font-size: .82rem;
    font-weight: 600;
    padding: .3rem .9rem;
    cursor: pointer;
  }
  .btn-save-custom:disabled { opacity: .4; cursor: default; }
  .btn-save-custom:not(:disabled):hover { opacity: .85; }

  /* ── Build-ID accordion ─────────────────────────────────────── */
  .build-list { display: flex; flex-direction: column; gap: .5rem; padding-bottom: 1.5rem; }

  .build-group {
    background: var(--surface);
    border-radius: 10px;
    overflow: hidden;
    border: 1px solid var(--border);
  }
  .build-group.open { border-color: var(--accent); }

  .build-header-row {
    display: flex;
    align-items: center;
  }
  .build-header {
    flex: 1;
    display: flex;
    align-items: center;
    gap: .65rem;
    padding: .75rem 1rem;
    background: none;
    border: none;
    color: var(--text);
    cursor: pointer;
    text-align: left;
    min-width: 0;
  }
  .build-header:hover { background: rgba(255,255,255,.04); }

  .btn-delete-custom {
    background: none;
    border: none;
    cursor: pointer;
    font-size: .95rem;
    padding: .75rem .85rem;
    opacity: .45;
    flex-shrink: 0;
    color: var(--text);
  }
  .btn-delete-custom:not(:disabled):hover { opacity: 1; color: #ff3b30; }
  .btn-delete-custom:disabled { opacity: .25; cursor: default; }

  .custom-badge {
    font-size: .68rem;
    font-weight: 700;
    background: rgba(255,179,0,.15);
    color: #ffb300;
    border-radius: 4px;
    padding: .05rem .4rem;
    flex-shrink: 0;
    letter-spacing: .02em;
  }

  .build-arrow { font-size: .7rem; color: var(--text-muted); flex-shrink: 0; width: .75rem; }
  .build-id {
    font-family: monospace;
    font-size: .88rem;
    font-weight: 600;
    letter-spacing: .03em;
    flex-shrink: 0;
  }
  .build-meta {
    font-size: .78rem;
    color: var(--text-muted);
    display: flex;
    align-items: center;
    gap: .35rem;
    flex: 1;
    flex-wrap: wrap;
  }
  .installed-count {
    background: rgba(52,199,89,.15);
    color: #34c759;
    border-radius: 4px;
    padding: .05rem .35rem;
    font-size: .72rem;
    font-weight: 600;
  }

  /* Truncate long credits strings so bad DB data can't dump walls of text */
  .credits-text {
    display: inline-block;
    max-width: 40ch;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    vertical-align: bottom;
  }

  /* ── Individual cheat sections ──────────────────────────────── */
  .section-list {
    border-top: 1px solid var(--border);
    display: flex;
    flex-direction: column;
  }

  .section-item {
    display: flex;
    align-items: center;
    padding: .55rem 1rem .55rem 2.1rem;
    gap: .75rem;
    border-bottom: 1px solid rgba(255,255,255,.04);
    transition: background .1s;
  }
  .section-item:last-child { border-bottom: none; }
  .section-item:hover { background: rgba(255,255,255,.03); }
  .section-item.installed { background: rgba(52,199,89,.06); }

  .section-name {
    flex: 1;
    font-size: .88rem;
  }
  .section-item.installed .section-name { color: #34c759; }

  .section-actions { display: flex; align-items: center; gap: .5rem; flex-shrink: 0; }

  .btn-install {
    background: var(--accent2, var(--accent));
    color: #fff;
    border: none;
    border-radius: 6px;
    padding: .3rem .75rem;
    font-size: .8rem;
    font-weight: 600;
    cursor: pointer;
    transition: opacity .15s;
    white-space: nowrap;
  }
  .btn-install:disabled { opacity: .4; cursor: default; }
  .btn-install:not(:disabled):hover { opacity: .85; }

  .installed-badge { font-size: .8rem; color: #34c759; font-weight: 600; }
  .install-msg { font-size: .78rem; color: var(--text-muted); }
  .install-msg.ok { color: #34c759; }

  /* ── Build ID detection ─────────────────────────────────────── */
  .detected-badge {
    font-size: .72rem;
    font-weight: 600;
    background: rgba(52,199,89,.15);
    color: #34c759;
    border-radius: 4px;
    padding: .05rem .4rem;
    flex-shrink: 0;
  }
  .detected-hint {
    margin: .5rem 1.5rem 0;
    background: rgba(90,130,255,.12);
    color: var(--text-muted);
    border-radius: 8px;
    padding: .45rem .9rem;
    font-size: .82rem;
  }
  .detected-hint code {
    font-size: .8rem;
    color: var(--text);
  }

  /* ── Scan Build ID ─────────────────────────────────────────────── */
  .scan-build-bar {
    display: flex;
    align-items: center;
    gap: .65rem;
    padding: .6rem 1.5rem;
    flex-shrink: 0;
  }
  .btn-scan-build {
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 7px;
    color: var(--text);
    font-size: .82rem;
    font-weight: 500;
    padding: .3rem .9rem;
    cursor: pointer;
    transition: opacity .15s, border-color .15s;
    white-space: nowrap;
  }
  .btn-scan-build:not(:disabled):hover { border-color: var(--accent); color: var(--accent); }
  .btn-scan-build:disabled { opacity: .45; cursor: default; }
  .scan-build-hint {
    font-size: .75rem;
    color: var(--text-muted, #888);
    margin: -.2rem 1.5rem .4rem;
    padding: 0;
  }
  .scan-build-error {
    font-size: .8rem;
    color: #ff3b30;
    flex: 1;
  }
</style>
