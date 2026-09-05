<script>
  import { onMount, untrack } from 'svelte';
  import * as backend from '../api/backend.js';
  import { cheatFileName, createInstalledIndex, groupCheatEntries, installedTupleKey } from '../domain/cheats.js';
  import { candidateKey, fallbackCandidate, gameCheatTarget, libraryCandidatesForGame, reconcileCandidate } from '../domain/library.js';
  import { createRequestRevisions } from '../domain/request-revisions.js';
  import Icon from './ui/Icon.svelte';
  import Dialog from './ui/Dialog.svelte';

  /** @typedef {import('../api/types.js').AppSettings} AppSettings */
  /** @typedef {import('../domain/library.js').PackageCandidate} PackageCandidate */
  /** @typedef {{tone: 'info'|'success'|'error', message: string}} Notice */
  /** @type {{game: import('../api/types.js').GameGroup|null, platform: import('../api/types.js').Platform,
   * settings: AppSettings, packageLibrary: import('../api/types.js').ManagedPackageLibrary|null,
   * androidPackageStatus: import('../api/types.js').PackageDiscoveryStatus|null,
   * pendingPicker: {id:number,kind:string}|null, contextRevision: number,
   * onback: ()=>void, onsettings: ()=>void, onpickandroid: (kind:'singlePackage')=>Promise<void>,
   * onnotify: (notice:Notice)=>void}} */
  let { game, platform, settings, packageLibrary, androidPackageStatus, pendingPicker,
    contextRevision, onback, onsettings, onpickandroid, onnotify } = $props();

  const id = $props.id();
  let target = $derived(gameCheatTarget(game));
  let activeTab = $state('catalog');
  let optionsOpen = $state(false);
  let versionOpen = $state(false);
  let optionsMenu = $state(/** @type {HTMLDetailsElement|null} */ (null));
  let imageFailed = $state(false);
  onMount(() => {
    /** @param {PointerEvent} event */
    const dismiss = event => { if (event.target instanceof Node && !optionsMenu?.contains(event.target)) optionsOpen = false; };
    /** @param {KeyboardEvent} event */
    const escape = event => {
      if (event.key === 'Escape' && optionsOpen) {
        event.preventDefault(); optionsOpen = false; optionsMenu?.querySelector('summary')?.focus();
      }
    };
    document.addEventListener('pointerdown', dismiss);
    document.addEventListener('keydown', escape);
    return () => { document.removeEventListener('pointerdown', dismiss); document.removeEventListener('keydown', escape); };
  });
  const requests = createRequestRevisions(['catalog', 'installed', 'fallback', 'mutation']);
  let catalog = $state(/** @type {import('../api/types.js').GameInfo|null} */ (null));
  let installed = $state(/** @type {import('../api/types.js').InstalledCheat[]} */ ([]));
  let catalogLoading = $state(false);
  let installedLoading = $state(false);
  let catalogError = $state('');
  let installedError = $state('');
  let actionError = $state('');
  let working = $state(false);
  let inspecting = $state(false);
  let chosen = $state(/** @type {PackageCandidate|null} */ (null));
  let fallback = $state(/** @type {PackageCandidate|null} */ (null));
  let customOpen = $state(false);
  let customBuild = $state('');
  let customContent = $state('');
  let confirmation = $state(/** @type {{title:string, message:string, action:()=>Promise<void>}|null} */ (null));
  let catalogHeading = $state(/** @type {HTMLElement|null} */ (null));
  let installedHeading = $state(/** @type {HTMLElement|null} */ (null));
  let candidates = $derived([
    ...(game && packageLibrary ? libraryCandidatesForGame(packageLibrary, game.baseTitleId) : []),
    ...(fallback ? [fallback] : []),
  ]);
  let groups = $derived(groupCheatEntries(catalog?.cheats ?? []));
  let installedIndex = $derived(createInstalledIndex(installed));
  let chosenBuild = $derived(chosen ? evidence(chosen).buildId.toUpperCase() : '');
  let visibleGroups = $derived(chosenBuild ? groups.filter(group => group.buildId === chosenBuild) : groups);

  /** @param {KeyboardEvent} event */
  function navigateTabs(event) {
    if (!['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key)) return;
    event.preventDefault();
    activeTab = event.key === 'Home' ? 'catalog' : event.key === 'End' ? 'installed'
      : activeTab === 'catalog' ? 'installed' : 'catalog';
    document.getElementById(`${id}-${activeTab}-tab`)?.focus();
  }

  /** @param {PackageCandidate} candidate */
  function versionLabel(candidate) {
    const value = evidence(candidate);
    const kind = value.contentKind === 'application' ? 'Base game' : 'Update';
    return kind;
  }

  /** @param {PackageCandidate} candidate */
  function evidence(candidate) { return candidate.source === 'library' ? candidate.package : candidate.metadata; }
  /** @param {unknown} error */
  function message(error) { return error instanceof Error ? error.message : String(error); }
  function context() {
    return { titleId: target?.titleId ?? '', expectedBaseTitleId: target?.baseTitleId ?? '', edenPresent: target?.installed === true, platform, settings: {...settings} };
  }
  /** @param {'catalog'|'installed'} resource @param {ReturnType<typeof context>} ctx */
  async function read(resource, ctx) {
    const token = requests.begin(resource);
    if (resource === 'catalog') { catalogLoading = true; catalogError = ''; }
    else { installedLoading = true; installedError = ''; }
    try {
      if (resource === 'catalog') {
        const result = await backend.searchCheats(ctx.titleId);
        if (requests.isCurrent(token)) catalog = result;
      } else {
        const result = ctx.edenPresent ? await backend.listInstalledCheats(ctx.platform, ctx.settings, ctx.titleId) : [];
        if (requests.isCurrent(token)) installed = result;
      }
    } catch (error) {
      if (requests.isCurrent(token)) {
        if (resource === 'catalog') catalogError = message(error);
        else installedError = message(error);
      }
    } finally {
      if (requests.isCurrent(token)) {
        if (resource === 'catalog') catalogLoading = false;
        else installedLoading = false;
      }
    }
  }

  $effect(() => {
    const titleId = target?.titleId;
    const edenPresent = target?.installed;
    const revision = contextRevision;
    untrack(() => {
      requests.beginSelection();
      activeTab = 'catalog'; optionsOpen = false; versionOpen = false; imageFailed = false;
      catalog = null; installed = []; chosen = null; fallback = null;
      catalogError = ''; installedError = ''; actionError = '';
      working = false; inspecting = false; customOpen = false; confirmation = null;
      customBuild = ''; customContent = ''; catalogLoading = false; installedLoading = false;
      if (titleId) {
        const ctx = context();
        void read('catalog', ctx); void read('installed', ctx);
      }
    });
    return () => { requests.beginSelection(); };
  });
  $effect(() => {
    const available = candidates;
    untrack(() => { chosen = reconcileCandidate(chosen, available); });
  });

  /** @param {'catalog'|'installed'} resource @param {()=>Promise<string>} action */
  async function mutate(resource, action) {
    if (working || !target) return;
    const ctx = context();
    const token = requests.begin('mutation');
    requests.invalidate(resource);
    if (resource === 'catalog') catalogLoading = false;
    else installedLoading = false;
    working = true; actionError = '';
    try {
      const result = await action();
      if (!requests.isCurrent(token)) return;
      onnotify({tone: 'success', message: result});
      customOpen = false; confirmation = null;
      await read(resource, ctx);
      if (requests.isCurrent(token)) (activeTab === 'catalog' ? catalogHeading : installedHeading)?.focus();
    } catch (error) {
      if (requests.isCurrent(token)) {
        actionError = message(error);
        await read(resource, ctx);
      }
    } finally {
      if (requests.isCurrent(token)) working = false;
    }
  }

  /** @param {string} buildId @param {import('../domain/cheats.js').CheatSection} section */
  function install(buildId, section) {
    if (!target?.installed) return;
    const ctx = context();
    const cheatName = cheatFileName(section.name, buildId);
    const action = () => mutate('installed', async () => {
      await backend.installCheat(ctx.platform, ctx.settings, {titleId:ctx.titleId, buildId, cheatName, content:section.content});
      return `Installed ${cheatName}. Enable it in Eden when ready.`;
    });
    if (installedIndex.has(installedTupleKey(buildId, cheatName))) {
      confirmation = {title:'Replace installed cheat?', message:`This overwrites ${cheatName} for Build ID ${buildId}.`, action};
    } else void action();
  }
  /** @param {import('../api/types.js').InstalledCheat} item */
  function removeInstalled(item) {
    const ctx = context();
    confirmation = {title:'Remove installed cheat?', message:`Remove ${item.cheatName} (${item.buildId}) from Eden? Its catalog entry will remain.`,
      action: () => mutate('installed', async () => {
        await backend.deleteInstalledCheat(ctx.platform, ctx.settings, {titleId:ctx.titleId,...item});
        return 'Installed cheat removed.';
      })};
  }
  function fetchOnline() {
    const ctx = context();
    void mutate('catalog', async () => {
      const added = await backend.fetchCheatsOnline(ctx.titleId, ctx.settings.apiToken);
      return added ? `Added ${added} catalog entries.` : 'No new entries added.';
    });
  }
  function clearFetched() {
    const ctx = context();
    confirmation = {title:'Clear downloaded entries?', message:'Remove downloaded catalog entries for this title. Custom entries and installed cheat files will remain.',
      action:()=>mutate('catalog', async()=>`${await backend.clearFetchedCheats(ctx.titleId)} downloaded entries removed.`)};
  }
  /** @param {number} entryId */
  function removeCustom(entryId) {
    confirmation = {title:'Delete custom entry?', message:'Delete this custom catalog entry and all sections within it. Installed files will remain.',
      action:()=>mutate('catalog', async()=>{await backend.deleteCustomCheat(entryId); return 'Custom entry deleted.';})};
  }
  function saveCustom() {
    const ctx = context();
    const buildId = customBuild.trim(); const content = customContent.trim();
    if (!buildId || !content) { actionError = 'Enter a Build ID and cheat content.'; return; }
    void mutate('catalog', async()=>{
      await backend.saveCustomCheat(ctx.titleId, buildId, content);
      return 'Custom entry saved. Install its sections separately.';
    });
  }
  async function inspect() {
    if (!target || inspecting) return;
    const ctx = context(); const token = requests.begin('fallback');
    inspecting = true; actionError = '';
    try {
      const path = ctx.platform === 'desktop' ? await backend.pickGamePackageFile() : '';
      if (path === null || !requests.isCurrent(token)) return;
      const result = await backend.inspectPackageForTitle(ctx.platform, ctx.settings, ctx.expectedBaseTitleId, path);
      if (requests.isCurrent(token)) {
        fallback = fallbackCandidate(result, ctx.platform === 'desktop' ? path.split(/[\\/]/).pop() : androidPackageStatus?.packageName);
        optionsOpen = false;
        onnotify({tone:'info',message:'Package inspected. Choose it in Game version to see matching cheats.'});
      }
    } catch (error) { if (requests.isCurrent(token)) actionError = message(error); }
    finally { if (requests.isCurrent(token)) inspecting = false; }
  }
  async function pickAndroid() {
    const token = requests.begin('fallback');
    try { await onpickandroid('singlePackage'); }
    catch (error) { if (requests.isCurrent(token)) actionError = message(error); }
  }
</script>

<section class="workspace" aria-label="Game workspace">
  {#if game && target}
    <header class="app-bar">
      <button class="md-icon-button back-control" aria-label="Back to library" onclick={onback}><Icon name="back" /></button>
      {#if game.baseImage && !imageFailed}
        <img class="game-art" src={game.baseImage} alt="" onerror={()=>imageFailed=true} />
      {:else}<span class="game-art placeholder" aria-hidden="true"><Icon name="game" size={36} /></span>{/if}
      <div class="game-identity">
        <h1 tabindex="-1" data-workspace-heading>{game.baseName || target.name || game.baseTitleId}</h1>
        <div class="game-meta"><p>{game.baseTitleId}</p>
        {#if game.baseInstalled || game.updates.some(entry => entry.installed)}
          <span class="presence"><Icon name="check" size={20} />Present in Eden</span>
        {:else}<small>Not present in Eden</small>
        {/if}
        </div>
      </div>
      <details class="options-menu" bind:this={optionsMenu} bind:open={optionsOpen}>
        <summary class="md-icon-button" aria-label="Game options"><Icon name="more" /></summary>
        <div class="game-options">{@render gameOptions()}</div>
      </details>
    </header>
    <div class="workspace-scroll">
        {#if actionError}<p class="error" role="alert">{actionError}</p>{/if}
        <div class="version-control">
          <button class="version-picker" aria-haspopup="dialog" aria-labelledby={`${id}-version-label ${id}-version-value`}
            disabled={working || !candidates.length} aria-describedby={`${id}-version-help`} onclick={()=>versionOpen=true}>
            <span class="version-label" id={`${id}-version-label`}>Game version</span>
            <span id={`${id}-version-value`}>{chosen ? versionLabel(chosen) : 'Please select a build'}</span><Icon name="expand" />
          </button>
          <div id={`${id}-version-help`} class="version-help">
            {#if chosenBuild}
              <span>Build {chosenBuild} · Does not change the version in Eden.</span>
            {:else if candidates.length}
              <span>Choose your base game or update to show matching cheats.</span>
            {:else}
              <span>Build ID not detected.</span>
              <button class="text-link" onclick={onsettings}>Set up version detection</button>
            {/if}
          </div>
        </div>

        <div class="tabs" role="tablist" aria-label="Game cheats">
          <button id={`${id}-catalog-tab`} type="button" role="tab" aria-selected={activeTab === 'catalog'}
            aria-controls={`${id}-catalog-panel`} tabindex={activeTab === 'catalog' ? 0 : -1} onkeydown={navigateTabs} onclick={()=>activeTab='catalog'}><span>Cheats</span></button>
          <button id={`${id}-installed-tab`} type="button" role="tab" aria-selected={activeTab === 'installed'}
            aria-controls={`${id}-installed-panel`} tabindex={activeTab === 'installed' ? 0 : -1} onkeydown={navigateTabs} onclick={()=>activeTab='installed'}><span>Installed{installedLoading ? '' : ` (${installed.length})`}</span></button>
        </div>

        <div class="tab-panel" id={`${id}-installed-panel`} role="tabpanel" tabindex="0" hidden={activeTab !== 'installed'} aria-labelledby={`${id}-installed-tab`} aria-busy={installedLoading}>
          <div class="section-heading"><h2 tabindex="-1" bind:this={installedHeading}>Installed in Eden</h2><button class="md-icon-button" aria-label="Refresh installed cheats" disabled={installedLoading || working} onclick={()=>read('installed',context())}><Icon name="refresh" /></button></div>
          <p class="support">All builds · Enable installed cheats in Eden.</p>
          {#if installedError}<p class="error" role="alert">{installedError}</p>{/if}
          {#if installedLoading}<div class="md-progress" role="progressbar" aria-label="Loading installed cheats"></div>{/if}
          {#each installed as item (installedTupleKey(item.buildId,item.cheatName))}
            <div class="file-row"><div>{item.cheatName}<small>{item.buildId}</small></div><button class="md-icon-button" aria-label={`Remove ${item.cheatName}`} disabled={working} onclick={()=>removeInstalled(item)}><Icon name="delete" /></button></div>
          {:else}{#if !installedLoading && !installedError}<p class="support">No cheat files installed for this title.</p>{/if}{/each}
        </div>

        <div class="tab-panel" id={`${id}-catalog-panel`} role="tabpanel" tabindex="0" hidden={activeTab !== 'catalog'} aria-labelledby={`${id}-catalog-tab`} aria-busy={catalogLoading}>
          <h2 class="md-sr-only" tabindex="-1" bind:this={catalogHeading}>Available cheats</h2>
          <div class="actions catalog-actions">
            {#if settings.apiToken}
              <button class="md-button md-button--tonal" disabled={working} onclick={fetchOnline}><Icon name="download" size={20} />Fetch online</button>
            {:else}
              <button class="md-button md-button--tonal" disabled={working} onclick={onsettings} aria-label="Connect Cheatslips to fetch online"><Icon name="key" size={20} />Connect source</button>
            {/if}
            <button class="md-button md-button--outlined" disabled={working} onclick={()=>{customBuild=chosenBuild;customContent='';actionError='';customOpen=true;}}><Icon name="add" size={20} />Custom cheat</button>
            <button class="md-icon-button refresh-catalog" aria-label="Refresh cheat catalog" disabled={catalogLoading || working} onclick={()=>read('catalog',context())}><Icon name="refresh" /></button>
          </div>
          {#if !target.installed}<p class="support">Open this game in Eden before installing cheats.</p>{/if}
          {#if catalogError}<p class="error" role="alert">{catalogError}</p>{/if}
          {#if catalogLoading || working}<div class="md-progress" role="progressbar" aria-label={working ? 'Saving changes' : 'Loading cheat catalog'}></div>{/if}
          {#each visibleGroups as group (group.buildId)}
            <details class="build-group" open={chosenBuild === group.buildId}>
              <summary><span>{group.sections.length} cheat{group.sections.length === 1 ? '' : 's'}<small>Build {group.buildId}{chosenBuild ? ' · Matches selected version' : ''}</small></span><Icon name="expand" /></summary>
              {#if group.credits}<p class="support">Credits: {group.credits}</p>{/if}
              {#each group.sections as section}
                {@const present = installedIndex.has(installedTupleKey(group.buildId,cheatFileName(section.name,group.buildId)))}
                <div class="cheat-row"><details><summary><Icon name="expand" size={20} /><span>{section.name}{section.custom ? ' · Custom' : ''}</span></summary><pre>{section.content}</pre></details>
                  <button class="md-button md-button--text" disabled={!target.installed || working || installedLoading || !!installedError} onclick={()=>install(group.buildId,section)}>{present ? 'Replace file' : 'Install'}</button></div>
              {/each}
              {#each group.customEntries as entry (entry.entryId)}
                <div class="file-row"><details><summary><Icon name="expand" size={20} /><span>Custom entry #{entry.entryId}</span></summary><pre>{entry.content}</pre></details><button class="md-icon-button" aria-label={`Delete custom entry ${entry.entryId}`} disabled={working} onclick={()=>removeCustom(entry.entryId)}><Icon name="delete" /></button></div>
              {/each}
            </details>
          {:else}{#if !catalogLoading && !catalogError}<p class="support">{chosenBuild ? 'No cheats for this version yet. Choose All builds to browse other versions.' : 'No cheats yet. Fetch online or add a custom cheat.'}</p>{/if}{/each}
        </div>
    </div>
  {:else}<div class="empty"><span class="empty-art"><Icon name="game" size={48} /></span><h1>Choose a game</h1><p>Browse its versions and manage cheats in one place.</p></div>{/if}
</section>

<Dialog open={versionOpen} title="Game version" onclose={()=>versionOpen=false}>
  <fieldset class="version-choices">
    <legend class="md-sr-only">Version to show cheats for</legend>
    <label><input type="radio" name={`${id}-version`} checked={!chosen} onchange={()=>{chosen=null;versionOpen=false;}} />
      <span>All builds<small>Browse every available cheat build.</small></span></label>
    {#each candidates as candidate (candidateKey(candidate))}
      <label><input type="radio" name={`${id}-version`} checked={chosen !== null && candidateKey(chosen) === candidateKey(candidate)}
        onchange={()=>{chosen=candidate;versionOpen=false;}} />
        <span>{versionLabel(candidate)}<small>{candidate.source === 'library' ? candidate.package.filename : candidate.label}</small><small>Build {evidence(candidate).buildId}</small></span></label>
    {/each}
  </fieldset>
  {#snippet actions()}<button class="md-button md-button--text" onclick={()=>versionOpen=false}>Close</button>{/snippet}
</Dialog>

{#snippet gameOptions()}
    <button class="md-button md-button--text" onclick={()=>{optionsOpen=false;onsettings();}}>Settings</button>
    {#if platform === 'android'}
      <button class="md-button md-button--text" disabled={!!pendingPicker || inspecting} onclick={pickAndroid}>Choose one package</button>
      <button class="md-button md-button--text" disabled={!androidPackageStatus?.ready || !!pendingPicker || inspecting} onclick={inspect}>{inspecting ? 'Inspecting…' : 'Inspect selected package'}</button>
      <p class="support">{androidPackageStatus?.packageName || 'No single package selected'}</p>
    {:else}<button class="md-button md-button--text" disabled={inspecting || !settings.prodKeysPath} onclick={inspect}>{inspecting ? 'Inspecting…' : 'Inspect one package'}</button>{/if}
    <button class="md-button md-button--text" disabled={working} onclick={()=>{optionsOpen=false;clearFetched();}}>Clear downloaded cheats</button>
    <small>Cheat file location · {target?.titleId ?? ''}</small>
    {#if actionError}<p class="error" role="alert">{actionError}</p>{/if}
{/snippet}

<Dialog open={customOpen} fullScreen title="Add custom cheat" onclose={()=>customOpen=false}>
  <form id="custom-cheat-form" onsubmit={(event)=>{event.preventDefault();saveCustom();}}>
    <p class="support">Save to the catalog first, then install individual named sections.</p>
    <label class="md-field">Build ID<input bind:value={customBuild} required disabled={working} /></label>
    <label class="md-field">Cheat content<textarea bind:value={customContent} required disabled={working} placeholder={'[Cheat name]\n04000000 00000000'}></textarea></label>
    {#if actionError}<p class="error" role="alert">{actionError}</p>{/if}
  </form>
  {#snippet actions()}<button class="md-button md-button--text" onclick={()=>customOpen=false}>Cancel</button><button class="md-button md-button--filled" type="submit" form="custom-cheat-form" disabled={working}>Save entry</button>{/snippet}
</Dialog>
<Dialog open={confirmation !== null} title={confirmation?.title ?? 'Confirm'} onclose={()=>confirmation=null}>
  <p>{confirmation?.message}</p>
  {#if actionError}<p class="error" role="alert">{actionError}</p>{/if}
  {#snippet actions()}<button class="md-button md-button--text" onclick={()=>confirmation=null}>Cancel</button><button class="md-button md-button--danger" disabled={working} onclick={()=>confirmation?.action()}>Confirm</button>{/snippet}
</Dialog>

<style>
  .workspace { display:flex; flex-direction:column; min-width:0; min-height:0; height:100%; background:var(--md-sys-color-surface); }
  .app-bar { display:flex; flex:none; align-items:center; gap:16px; padding:24px max(24px,env(safe-area-inset-right)) 16px max(24px,env(safe-area-inset-left)); padding-top:max(24px,env(safe-area-inset-top)); }
  .app-bar > div { flex:1; min-width:0; }
  h1 { font:var(--md-sys-typescale-headline-small); overflow-wrap:anywhere; }
  h2 { font:var(--md-sys-typescale-title-medium); }
  .app-bar p, small, .support { color:var(--md-sys-color-on-surface-variant); font:var(--md-sys-typescale-body-medium); overflow-wrap:anywhere; }
  small { display:block; font:var(--md-sys-typescale-body-small); }
  .support { margin-block:8px 16px; }
  .workspace-scroll { flex:1; min-height:0; overflow:auto; padding:8px var(--md-sys-layout-gutter) max(24px,env(safe-area-inset-bottom)); overscroll-behavior:contain; scrollbar-gutter:stable; }
  .workspace-scroll > * { max-width:960px; margin-inline:auto; }
  .app-bar { width:100%; max-width:1008px; margin-inline:auto; }
  .tab-panel { margin-bottom:24px; }
  .section-heading { display:flex; align-items:center; justify-content:space-between; gap:8px; } .actions { display:flex; flex-wrap:wrap; align-items:center; gap:8px; }
  .game-art { width:80px; height:80px; flex:0 0 80px; object-fit:cover; border-radius:var(--md-sys-shape-corner-medium); background:var(--md-sys-color-surface-container-highest); }
  .placeholder { display:grid; place-items:center; color:var(--md-sys-color-on-surface-variant); }
  .game-identity { display:grid; gap:4px; }
  .game-meta { display:flex; align-items:center; flex-wrap:wrap; gap:4px 16px; }
  .presence { display:inline-flex; align-items:center; gap:4px; font:var(--md-sys-typescale-body-small); color:var(--md-sys-color-on-surface-variant); }
  .version-control { margin-bottom:16px; }
  .version-label { color:var(--md-sys-color-on-surface-variant); font:var(--md-sys-typescale-label-medium); }
  .version-picker { display:flex; align-items:center; gap:12px; width:100%; min-height:48px; padding:8px 16px; border:1px solid var(--md-sys-color-outline); border-radius:var(--md-sys-shape-corner-small); color:var(--md-sys-color-on-surface); background:none; text-align:start; font:var(--md-sys-typescale-body-large); }
  .version-picker > span:nth-child(2) { flex:1; min-width:0; overflow-wrap:anywhere; }
  .version-picker:hover:not(:disabled) { background:rgb(var(--md-sys-color-on-surface-rgb) / 0.08); }
  .version-picker:disabled { color:var(--md-sys-color-on-surface-variant); border-color:var(--md-sys-color-outline-variant); }
  .version-choices { margin:0; padding:0; border:0; }
  .version-choices label { display:flex; align-items:center; gap:16px; padding:16px 0; min-height:64px; border-bottom:1px solid var(--md-sys-color-outline-variant); cursor:pointer; }
  .version-choices input { flex:none; width:20px; height:20px; accent-color:var(--md-sys-color-primary); }
  .version-choices label > span { min-width:0; overflow-wrap:anywhere; }
  .version-help { display:flex; align-items:center; flex-wrap:wrap; gap:0 8px; padding-top:8px; color:var(--md-sys-color-on-surface-variant); font:var(--md-sys-typescale-body-small); overflow-wrap:anywhere; }
  .text-link { border:0; padding:0; background:none; color:var(--md-sys-color-primary); font:inherit; text-decoration:underline; text-underline-offset:3px; min-height:32px; }
  .tabs { display:flex; border-bottom:1px solid var(--md-sys-color-outline-variant); margin-bottom:16px; }
  .tabs button { flex:1; display:flex; align-items:stretch; justify-content:center; min-height:48px; border:0; padding:0 24px; background:none; color:var(--md-sys-color-on-surface-variant); font:var(--md-sys-typescale-title-small); }
  .tabs button > span { position:relative; display:flex; align-items:center; }
  .tabs button[aria-selected="true"] { color:var(--md-sys-color-primary); }
  .tabs button[aria-selected="true"] > span::after { position:absolute; content:''; inset-inline:-4px; bottom:0; height:3px; border-radius:3px 3px 0 0; background:var(--md-sys-color-primary); }
  .tabs button:hover { background:rgb(var(--md-sys-color-primary-rgb) / 0.08); }
  .catalog-actions { margin-bottom:8px; }
  .refresh-catalog { margin-inline-start:auto; }
  .options-menu { position:relative; flex:none; }
  .options-menu > summary { list-style:none; }
  .options-menu > summary::-webkit-details-marker { display:none; }
  .game-options { position:absolute; z-index:3; right:0; top:100%; display:grid; gap:4px; width:min(320px,calc(100vw - 32px)); max-height:calc(100dvh - 160px); overflow:auto; padding:12px; border-radius:12px; background:var(--md-sys-color-surface-container-high); box-shadow:var(--md-sys-elevation-level-3); }
  .game-options > button { justify-content:flex-start; border-radius:4px; color:var(--md-sys-color-on-surface); white-space:normal; text-align:start; }
  .file-row, .cheat-row { display:flex; align-items:center; justify-content:space-between; gap:8px; padding:8px 0; border-bottom:1px solid var(--md-sys-color-outline-variant); } .file-row > div,.file-row > details,.cheat-row > details { flex:1; min-width:0; overflow-wrap:anywhere; }
  .build-group { margin-top:12px; padding:0 16px; border-radius:var(--md-sys-shape-corner-medium); background:var(--md-sys-color-surface-container-low); }
  .build-group[open] { padding-bottom:8px; }
  summary { cursor:pointer; min-height:48px; align-content:center; overflow-wrap:anywhere; list-style:none; }
  summary::-webkit-details-marker { display:none; }
  .build-group > summary { display:flex; align-items:center; justify-content:space-between; gap:8px; min-height:72px; font:var(--md-sys-typescale-title-medium); }
  .build-group > summary span { min-width:0; }
  .cheat-row summary, .file-row summary { display:flex; align-items:center; gap:8px; font:var(--md-sys-typescale-body-large); }
  summary :global(svg) { color:var(--md-sys-color-on-surface-variant); transition:transform var(--md-sys-motion-duration-enter) var(--md-sys-motion-easing-standard); }
  details[open] > summary > :global(svg) { transform:rotate(180deg); }
  .file-row:last-child, .cheat-row:last-child { border-bottom:0; }
  pre { overflow:auto; max-height:240px; margin:8px 0; padding:12px; font:12px/1.5 ui-monospace,monospace; background:var(--md-sys-color-surface-container-lowest); border-radius:8px; }
  .empty { display:flex; flex:1; flex-direction:column; align-items:center; justify-content:center; text-align:center; gap:16px; padding:32px; color:var(--md-sys-color-on-surface-variant); }
  .empty-art { display:grid; place-items:center; width:96px; height:96px; border-radius:28px; color:var(--md-sys-color-on-secondary-container); background:var(--md-sys-color-secondary-container); }
  .empty p { max-width:320px; font:var(--md-sys-typescale-body-medium); }
  .error { padding:12px; margin-block:8px; overflow-wrap:anywhere; border-radius:8px; color:var(--md-sys-color-on-error-container); background:var(--md-sys-color-error-container); }
  form { display:grid; gap:16px; }
  @media (min-width:900px) and (min-height:600px) { .back-control { display:none; } }
  @media (max-width:599px), (max-height:599px) {
    .app-bar { gap:12px; padding:12px max(16px,env(safe-area-inset-right)) 12px max(8px,env(safe-area-inset-left)); padding-top:max(12px,env(safe-area-inset-top)); }
    .game-art { width:56px; height:56px; flex-basis:56px; border-radius:12px; }
    h1 { font:var(--md-sys-typescale-title-large); }
    .app-bar p { font-size:12px; }
    .version-control { margin-bottom:8px; }
    .tabs { margin-bottom:12px; }
  }
  @media (max-width:599px) {
    .catalog-actions { gap:4px; }
    .catalog-actions .md-button { padding-inline:12px; }
    .game-meta { gap:2px 8px; }
    .game-identity h1 { font:var(--md-sys-typescale-title-medium); }
    .version-label { max-width:72px; }
    .tabs button { padding-inline:16px; }
  }
</style>
