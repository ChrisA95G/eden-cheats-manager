<script>
  import { untrack } from 'svelte';
  import * as backend from '../api/backend.js';
  import { cheatFileName, createInstalledIndex, groupCheatEntries, installedTupleKey } from '../domain/cheats.js';
  import { candidateKey, fallbackCandidate, libraryCandidatesForTitle, reconcileCandidate } from '../domain/library.js';
  import { createRequestRevisions } from '../domain/request-revisions.js';
  import Icon from './ui/Icon.svelte';
  import Dialog from './ui/Dialog.svelte';

  /** @typedef {import('../api/types.js').TitleEntry} TitleEntry */
  /** @typedef {import('../api/types.js').AppSettings} AppSettings */
  /** @typedef {import('../domain/library.js').PackageCandidate} PackageCandidate */
  /** @typedef {{tone: 'info'|'success'|'error', message: string}} Notice */
  /** @type {{target: TitleEntry|null, platform: import('../api/types.js').Platform,
   * settings: AppSettings, packageLibrary: import('../api/types.js').ManagedPackageLibrary|null,
   * androidPackageStatus: import('../api/types.js').PackageDiscoveryStatus|null,
   * pendingPicker: {id:number,kind:string}|null, contextRevision: number,
   * onback: ()=>void, onsettings: ()=>void, onpickandroid: (kind:'singlePackage')=>Promise<void>,
   * onnotify: (notice:Notice)=>void}} */
  let { target, platform, settings, packageLibrary, androidPackageStatus, pendingPicker,
    contextRevision, onback, onsettings, onpickandroid, onnotify } = $props();

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
    ...(target && packageLibrary ? libraryCandidatesForTitle(packageLibrary, target.titleId) : []),
    ...(fallback ? [fallback] : []),
  ]);
  let groups = $derived(groupCheatEntries(catalog?.cheats ?? []));
  let installedIndex = $derived(createInstalledIndex(installed));
  let chosenBuild = $derived(chosen ? evidence(chosen).buildId.toUpperCase() : '');

  /** @param {PackageCandidate} candidate */
  function evidence(candidate) { return candidate.source === 'library' ? candidate.package : candidate.metadata; }
  /** @param {unknown} error */
  function message(error) { return error instanceof Error ? error.message : String(error); }
  function context() {
    return { titleId: target?.titleId ?? '', expectedBaseTitleId: target?.baseTitleId ?? '', platform, settings: {...settings} };
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
        const result = await backend.listInstalledCheats(ctx.platform, ctx.settings, ctx.titleId);
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
    const revision = contextRevision;
    untrack(() => {
      requests.beginSelection();
      catalog = null; installed = []; chosen = null; fallback = null;
      catalogError = ''; installedError = ''; actionError = '';
      working = false; inspecting = false; customOpen = false; confirmation = null;
      customBuild = ''; customContent = ''; catalogLoading = false; installedLoading = false;
      if (titleId && target?.category !== 'dlc') {
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
      if (requests.isCurrent(token)) (resource === 'catalog' ? catalogHeading : installedHeading)?.focus();
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
        onnotify({tone:'info',message:'Package inspected. Select it below to compare builds.'});
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
  {#if target}
    <header class="app-bar">
      <button class="md-icon-button back-control" aria-label="Back to library" onclick={onback}><Icon name="back" /></button>
      <div><h1 tabindex="-1" data-workspace-heading>{target.name || target.titleId}</h1><p>{target.category === 'base' ? 'Base game' : target.category === 'update' ? 'Update' : 'DLC'} · {target.titleId}</p></div>
      <button class="md-icon-button" aria-label="Open settings" onclick={onsettings}><Icon name="settings" /></button>
    </header>
    <div class="workspace-scroll">
      {#if target.category === 'dlc'}
        <div class="empty"><Icon name="info" size={36} /><h2>Cheats are not supported for DLC</h2><p>Choose a base game or update from the library.</p><button class="md-button md-button--tonal" onclick={onback}>Back to library</button></div>
      {:else}
        {#if actionError}<p class="error" role="alert">{actionError}</p>{/if}
        <section class="package-section" aria-labelledby="package-title">
          <div class="section-heading"><h2 id="package-title">Choose a package version</h2><button class="md-button md-button--text" onclick={onsettings}>Configure</button></div>
          <p class="support">Compare cheats with a package you own. This does not identify which version Eden is running.</p>
          {#if candidates.length}
            <fieldset class="candidates"><legend class="md-sr-only">Package version to compare</legend>
              <label class="candidate"><input type="radio" name="package" checked={!chosen} onchange={()=>chosen=null} /><span>Browse all builds <small>Compatibility unverified</small></span></label>
              {#each candidates as candidate}
                {@const value = evidence(candidate)}
                <label class="candidate"><input type="radio" name="package" checked={chosen !== null && candidateKey(chosen) === candidateKey(candidate)} onchange={()=>chosen=candidate} />
                  <span>{candidate.source === 'library' ? candidate.package.filename : candidate.label}<small>{value.contentKind} · version {value.version} · {value.buildId}</small></span>
                </label>
              {/each}
            </fieldset>
          {:else}<p class="support">No package candidates available. You can still browse and manage cheat files.</p>{/if}
          <div class="actions">
            {#if platform === 'android'}
              <button class="md-button md-button--outlined" disabled={!!pendingPicker || inspecting} onclick={pickAndroid}>Choose one package</button>
              <button class="md-button md-button--tonal" disabled={!androidPackageStatus?.ready || !!pendingPicker || inspecting} onclick={inspect}>{inspecting ? 'Inspecting…' : 'Inspect selected package'}</button>
              <p class="support">{androidPackageStatus?.packageName || 'No single package selected'}</p>
            {:else}<button class="md-button md-button--outlined" disabled={inspecting || !settings.prodKeysPath} onclick={inspect}>{inspecting ? 'Inspecting…' : 'Inspect one package'}</button>{/if}
          </div>
        </section>

        <section aria-labelledby="installed-title" aria-busy={installedLoading}>
          <div class="section-heading"><h2 id="installed-title" tabindex="-1" bind:this={installedHeading}>Installed files <span>{installed.length}</span></h2><button class="md-icon-button" aria-label="Refresh installed cheats" disabled={installedLoading || working} onclick={()=>read('installed',context())}><Icon name="refresh" /></button></div>
          <p class="support">Files present in Eden. Presence does not mean enabled or compatible.</p>
          {#if installedError}<p class="error" role="alert">{installedError}</p>{/if}
          {#if installedLoading}<div class="md-progress" role="progressbar" aria-label="Loading installed cheats"></div>{/if}
          {#each installed as item (installedTupleKey(item.buildId,item.cheatName))}
            <div class="file-row"><div>{item.cheatName}<small>{item.buildId}</small></div><button class="md-icon-button" aria-label={`Remove ${item.cheatName}`} disabled={working} onclick={()=>removeInstalled(item)}><Icon name="delete" /></button></div>
          {:else}{#if !installedLoading && !installedError}<p class="support">No cheat files installed for this title.</p>{/if}{/each}
        </section>

        <section aria-labelledby="catalog-title" aria-busy={catalogLoading}>
          <div class="section-heading"><h2 id="catalog-title" tabindex="-1" bind:this={catalogHeading}>Cheat catalog</h2><button class="md-icon-button" aria-label="Refresh cheat catalog" disabled={catalogLoading || working} onclick={()=>read('catalog',context())}><Icon name="refresh" /></button></div>
          <div class="actions">
            <button class="md-button md-button--tonal" disabled={working || !settings.apiToken} onclick={fetchOnline}><Icon name="download" size={18} />Fetch online</button>
            <button class="md-button md-button--outlined" disabled={working} onclick={()=>{customBuild=chosenBuild;customContent='';actionError='';customOpen=true;}}><Icon name="add" size={18} />Custom cheat</button>
            <button class="md-button md-button--text" disabled={working} onclick={clearFetched}>Clear downloaded</button>
          </div>
          {#if !settings.apiToken}<p class="support">Add a Cheatslips token in Settings to fetch online.</p>{/if}
          {#if catalogError}<p class="error" role="alert">{catalogError}</p>{/if}
          {#if catalogLoading || working}<div class="md-progress" role="progressbar" aria-label={working ? 'Saving changes' : 'Loading cheat catalog'}></div>{/if}
          {#each groups as group (group.buildId)}
            <details class="build-group" open={chosenBuild === group.buildId}>
              <summary><span>Build {group.buildId}<small>{group.sections.length} sections · {chosenBuild ? chosenBuild === group.buildId ? 'Matches selected package' : 'Other build' : 'Compatibility unverified'}</small></span><Icon name="expand" /></summary>
              {#if group.credits}<p class="support">Credits: {group.credits}</p>{/if}
              {#each group.sections as section}
                {@const present = installedIndex.has(installedTupleKey(group.buildId,cheatFileName(section.name,group.buildId)))}
                <div class="cheat-row"><details><summary>{section.name}{section.custom ? ' · Custom' : ''}</summary><pre>{section.content}</pre></details>
                  <button class="md-button md-button--text" disabled={working || installedLoading || !!installedError} onclick={()=>install(group.buildId,section)}>{present ? 'Replace file' : 'Install'}</button></div>
              {/each}
              {#each group.customEntries as entry (entry.entryId)}
                <div class="file-row"><details><summary>Custom entry #{entry.entryId}</summary><pre>{entry.content}</pre></details><button class="md-icon-button" aria-label={`Delete custom entry ${entry.entryId}`} disabled={working} onclick={()=>removeCustom(entry.entryId)}><Icon name="delete" /></button></div>
              {/each}
            </details>
          {:else}{#if !catalogLoading && !catalogError}<p class="support">No local cheats for this title. Fetch online or add a custom entry.</p>{/if}{/each}
        </section>
      {/if}
    </div>
  {:else}<div class="empty"><Icon name="game" size={48} /><h1>Your next game, ready to tweak</h1><p>Choose a title from your library to manage its cheats.</p></div>{/if}
</section>

<Dialog open={customOpen} title="Add custom cheat" onclose={()=>customOpen=false}>
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
  .workspace { display:flex; flex-direction:column; min-width:0; min-height:0; height:100%; background:var(--md-sys-color-surface-container-low); }
  .app-bar { display:flex; flex:none; align-items:center; gap:8px; min-height:64px; padding:8px max(16px,env(safe-area-inset-right)) 8px max(8px,env(safe-area-inset-left)); padding-top:max(8px,env(safe-area-inset-top)); }
  .app-bar > div { flex:1; min-width:0; } h1 { font-size:22px; font-weight:400; overflow-wrap:anywhere; } h2 { font-size:20px; font-weight:500; } h2 span { color:var(--md-sys-color-on-surface-variant); font-size:14px; }
  .app-bar p, small, .support { color:var(--md-sys-color-on-surface-variant); font-size:14px; overflow-wrap:anywhere; } small { display:block; font-size:12px; } .support { margin-block:8px 16px; }
  .workspace-scroll { flex:1; min-height:0; overflow:auto; padding:8px var(--md-sys-layout-gutter) max(24px,env(safe-area-inset-bottom)); overscroll-behavior:contain; }
  .workspace-scroll > section { margin-bottom:24px; } .package-section { padding:16px; border-radius:16px; background:var(--md-sys-color-surface-container); }
  .section-heading { display:flex; align-items:center; justify-content:space-between; gap:8px; } .actions { display:flex; flex-wrap:wrap; align-items:center; gap:8px; }
  .candidates { margin:16px 0; border:0; padding:0; } .candidate { display:flex; align-items:center; gap:12px; min-height:56px; padding:10px 8px; cursor:pointer; border-bottom:1px solid var(--md-sys-color-outline-variant); } .candidate input { flex:none; accent-color:var(--md-sys-color-primary); width:20px; height:20px; } .candidate span { min-width:0; overflow-wrap:anywhere; }
  .file-row, .cheat-row { display:flex; align-items:center; justify-content:space-between; gap:8px; padding:8px 0; border-bottom:1px solid var(--md-sys-color-outline-variant); } .file-row > div,.file-row > details,.cheat-row > details { flex:1; min-width:0; overflow-wrap:anywhere; }
  .build-group { margin-top:12px; padding:0 16px 8px; border-radius:16px; background:var(--md-sys-color-surface-container); } summary { cursor:pointer; min-height:48px; align-content:center; overflow-wrap:anywhere; } .build-group > summary { display:flex; align-items:center; justify-content:space-between; gap:8px; min-height:72px; } .build-group > summary span { min-width:0; }
  pre { overflow:auto; max-height:240px; margin:8px 0; padding:12px; font:12px/1.5 ui-monospace,monospace; background:var(--md-sys-color-surface-container-lowest); border-radius:8px; }
  .empty { display:flex; flex:1; flex-direction:column; align-items:center; justify-content:center; text-align:center; gap:16px; padding:32px; color:var(--md-sys-color-on-surface-variant); }
  .error { padding:12px; margin-block:8px; overflow-wrap:anywhere; border-radius:8px; color:var(--md-sys-color-on-error-container); background:var(--md-sys-color-error-container); }
  form { display:grid; gap:16px; }
  @media (min-width:900px) and (min-height:600px) { .back-control { display:none; } }
  @media (max-height:599px) { .app-bar { min-height:56px; } .package-section { padding:12px; } }
</style>
