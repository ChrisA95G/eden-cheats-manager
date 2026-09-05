<script>
  import { onMount, tick, untrack } from 'svelte';
  import * as backend from '$lib/api/backend.js';
  import { createLibraryState, normalizeTitleId, reduceLibraryState } from '$lib/domain/library.js';
  import SetupScreen from '$lib/components/SetupScreen.svelte';
  import SettingsDialog from '$lib/components/SettingsDialog.svelte';
  import LibraryPane from '$lib/components/LibraryPane.svelte';
  import GameWorkspace from '$lib/components/GameWorkspace.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';

  /** @typedef {import('$lib/api/types.js').AppSettings} AppSettings */
  let platform = $state(/** @type {import('$lib/api/types.js').Platform} */ ('desktop'));
  let settings = $state(/** @type {AppSettings|null} */ (null));
  let loading = $state(true);
  let bootstrapError = $state('');
  let detectedPcLoadDir = $state('');
  let edenAccess = $state(/** @type {import('$lib/api/types.js').EdenLoadAccessStatus|null} */ (null));
  let packageStatus = $state(/** @type {import('$lib/api/types.js').PackageDiscoveryStatus|null} */ (null));
  let gameLibraryStatus = $state(/** @type {import('$lib/api/types.js').GameLibraryStatus|null} */ (null));
  let statusError = $state('');
  let checkingStatus = $state(false);
  let pendingPicker = $state(/** @type {{id:number,kind:'edenLoad'|'prodKeys'|'packageLibrary'|'singlePackage'}|null} */ (null));
  let library = $state(createLibraryState());
  let refreshPhase = $state(/** @type {'idle'|'loading'|'error'} */ ('idle'));
  let selectedTitleId = $state('');
  let pane = $state('library');
  let settingsOpen = $state(false);
  let saving = $state(false);
  let contextRevision = $state(0);
  let notice = $state(/** @type {{tone:'info'|'success'|'error',message:string}|null} */ (null));
  let ready = $derived(!!settings && (platform === 'android' ? edenAccess?.ready === true : settings.onboardingDone));
  let target = $derived(library.games.flatMap(group=>[
    ...(group.baseGame ? [group.baseGame] : []), ...group.updates, ...group.dlcs,
  ]).find(entry=>normalizeTitleId(entry.titleId) === normalizeTitleId(selectedTitleId)) ?? null);
  let alive = false;
  let scanRevision = 0;
  let statusRevision = 0;
  let pickerId = 0;
  let started = false;
  /** @type {ReturnType<typeof setTimeout>|undefined} */
  let resumeTimer;
  /** @type {ReturnType<typeof setTimeout>|undefined} */
  let noticeTimer;

  /** @param {unknown} cause */
  function errorMessage(cause) { return cause instanceof Error ? cause.message : String(cause); }
  /** @param {{tone:'info'|'success'|'error',message:string}} value */
  function notify(value) {
    notice = value; clearTimeout(noticeTimer);
    noticeTimer = setTimeout(()=>notice=null, 6500);
  }
  function navigationState() { return {ecm:1,pane,selectedTitleId}; }
  /** @param {string} titleId */
  async function selectTitle(titleId) {
    const push = pane !== 'workspace';
    selectedTitleId = titleId;
    if (push) history.replaceState({...history.state,...navigationState()},'');
    pane = 'workspace';
    if (push) history.pushState({...history.state,...navigationState()},'');
    else history.replaceState({...history.state,...navigationState()},'');
    await tick();
    if (!matchMedia('(min-width:900px) and (min-height:600px)').matches) {
      /** @type {HTMLElement|null} */ (document.querySelector('[data-workspace-heading]'))?.focus();
    }
  }
  function backToLibrary() { if (pane === 'workspace') history.back(); }
  /** @param {PopStateEvent} event */
  async function navigateBack(event) {
    if (event.state?.ecm !== 1) return;
    pane = event.state.pane ?? 'library';
    selectedTitleId = event.state.selectedTitleId ?? '';
    await tick();
    if (pane === 'library') {
      /** @type {HTMLElement|null} */ (document.querySelector('[data-title-id="' + CSS.escape(selectedTitleId) + '"]'))?.focus();
    }
  }
  function reconcileTitle() {
    const found = library.games.some(group=>[group.baseGame,...group.updates,...group.dlcs]
      .some(entry=>entry && normalizeTitleId(entry.titleId) === normalizeTitleId(selectedTitleId)));
    if (selectedTitleId && !found) {
      selectedTitleId = ''; pane = 'library';
      history.replaceState({...history.state,...navigationState()},'');
    }
  }
  async function refreshLibrary() {
    if (!ready || !alive) return;
    const revision = ++scanRevision;
    refreshPhase = 'loading';
    try {
      const snapshot = await backend.scanManagedLibrary(platform);
      if (!alive || revision !== scanRevision) return;
      library = reduceLibraryState(library,{type:'refreshSucceeded',snapshot});
      refreshPhase = 'idle'; reconcileTitle();
    } catch (cause) {
      if (!alive || revision !== scanRevision) return;
      library = reduceLibraryState(library,{type:'refreshFailed',error:errorMessage(cause)});
      refreshPhase = 'error';
    }
  }
  async function startLibrary() {
    if (started) return;
    started = true;
    const revision = ++scanRevision;
    refreshPhase = 'loading';
    try {
      const games = await backend.getCachedGames(platform);
      if (!alive || revision !== scanRevision) return;
      library = reduceLibraryState(library,{type:'cacheLoaded',games});
    } catch { /* An unreadable cache does not prevent a fresh scan. */ }
    if (alive && revision === scanRevision) await refreshLibrary();
  }
  $effect(()=>{
    if (ready && !loading) untrack(()=>{void startLibrary();});
    else if (!loading) untrack(()=>{
      scanRevision++; started = false; contextRevision++;
      library = createLibraryState(); selectedTitleId = ''; pane = 'library';
      settingsOpen = false;
    });
  });

  async function refreshAndroidStatus() {
    if (platform !== 'android') return;
    const revision = ++statusRevision;
    checkingStatus = true;
    const results = await Promise.allSettled([
      backend.getEdenLoadAccessStatus(),backend.getPackageDiscoveryStatus(),backend.getGameLibraryStatus(),
    ]);
    if (!alive || revision !== statusRevision) return;
    const [eden,keys,packages] = results;
    if (eden.status === 'fulfilled') edenAccess = eden.value;
    if (keys.status === 'fulfilled') packageStatus = keys.value;
    if (packages.status === 'fulfilled') gameLibraryStatus = packages.value;
    statusError = results.flatMap(result=>result.status === 'rejected' ? [errorMessage(result.reason)] : []).join('\n');
    checkingStatus = false;
  }
  function resumed() {
    if (document.visibilityState !== 'visible' || platform !== 'android') return;
    clearTimeout(resumeTimer);
    resumeTimer = setTimeout(async()=>{
      const picker = pendingPicker;
      await refreshAndroidStatus();
      if (!alive) return;
      if (picker && pendingPicker?.id === picker.id) {
        pendingPicker = null;
        if (picker.kind !== 'singlePackage') {
          contextRevision++;
          if (started) void refreshLibrary();
        }
      }
    },250);
  }
  /** @param {'edenLoad'|'prodKeys'|'packageLibrary'|'singlePackage'} kind */
  async function pickAndroid(kind) {
    if (pendingPicker) return;
    const picker = {id:++pickerId,kind}; pendingPicker = picker;
    try {
      const actions = {edenLoad:backend.selectEdenLoadDirectory,prodKeys:backend.selectProdKeysDocument,
        packageLibrary:backend.selectGameLibraryDirectory,singlePackage:backend.selectGamePackageDocument};
      await actions[kind]();
    } catch (cause) { if (pendingPicker?.id === picker.id) pendingPicker=null; throw cause; }
  }
  /** @param {'edenLoad'|'prodKeys'|'packageLibrary'} kind @param {string} path */
  function pickDesktop(kind,path) {
    const actions = {edenLoad:backend.pickEdenLoadDirectory,prodKeys:backend.pickProdKeysFile,
      packageLibrary:backend.pickPackageLibraryDirectory};
    return actions[kind](path);
  }
  /** @param {AppSettings} next */
  async function save(next) {
    if (saving) return;
    saving = true;
    try {
      await backend.saveSettings(next);
      if (!alive) return;
      settings = next; contextRevision++; settingsOpen = false;
      notify({tone:'success',message:'Settings saved.'});
      if (started) void refreshLibrary();
    } finally { saving = false; }
  }
  async function bootstrap() {
    loading = true; bootstrapError = '';
    try {
      const result = await backend.loadBootstrap();
      if (!alive) return;
      platform=result.platform; settings=result.settings; edenAccess=result.edenAccess; statusError=result.edenAccessError;
      if (platform === 'android') await refreshAndroidStatus();
      else if (!settings.pcLoadDir) {
        try { detectedPcLoadDir = await backend.detectPcLoadDir(); } catch { /* Manual setup remains available. */ }
      }
    } catch (cause) { if (alive) bootstrapError = errorMessage(cause); }
    finally { if (alive) loading=false; }
  }
  onMount(()=>{
    alive=true;
    history.replaceState({...history.state,...navigationState()},'');
    window.addEventListener('popstate',navigateBack);
    window.addEventListener('focus',resumed);
    document.addEventListener('visibilitychange',resumed);
    void bootstrap();
    return ()=>{
      alive=false; scanRevision++; statusRevision++;
      clearTimeout(resumeTimer); clearTimeout(noticeTimer);
      window.removeEventListener('popstate',navigateBack);
      window.removeEventListener('focus',resumed);
      document.removeEventListener('visibilitychange',resumed);
    };
  });
</script>

{#if loading}
  <main class="startup" aria-busy="true"><Icon name="game" size={40}/><h1>Eden Cheats Manager</h1><p>Opening your library…</p><div class="md-progress" role="progressbar" aria-label="Loading app"></div></main>
{:else if bootstrapError || !settings}
  <main class="startup"><h1>Unable to open the app</h1><p role="alert">{bootstrapError}</p><button class="md-button md-button--filled" onclick={bootstrap}>Try again</button></main>
{:else if !ready}
  <SetupScreen {platform} {settings} {detectedPcLoadDir} {edenAccess} busy={saving || checkingStatus || !!pendingPicker}
    error={statusError} onsubmit={save} onpickdesktop={pickDesktop} onpickandroid={()=>pickAndroid('edenLoad')}
    onretryandroid={async()=>{pendingPicker=null;await refreshAndroidStatus();}} />
{:else}
  <main class="app-shell" class:show-workspace={pane === 'workspace'}>
    <div class="library-slot"><LibraryPane games={library.games} packageLibrary={library.packageLibrary} {refreshPhase}
      refreshError={library.refreshError} {selectedTitleId} onselect={selectTitle} onrefresh={refreshLibrary} onsettings={()=>settingsOpen=true}/></div>
    <div class="workspace-slot"><GameWorkspace {target} {platform} {settings} packageLibrary={library.packageLibrary}
      androidPackageStatus={packageStatus} {pendingPicker} {contextRevision} onback={backToLibrary}
      onsettings={()=>settingsOpen=true} onpickandroid={pickAndroid} onnotify={notify}/></div>
  </main>
  <SettingsDialog open={settingsOpen} {platform} {settings} {edenAccess} {packageStatus} {gameLibraryStatus}
    {pendingPicker} {saving} error={statusError} onsave={save} onclose={()=>settingsOpen=false}
    onpickdesktop={pickDesktop} onpickandroid={pickAndroid} onretryandroid={refreshAndroidStatus}
    ontesteden={backend.testEdenLoadDirectory} onrevealapplog={()=>backend.revealAppLog(platform)}/>
{/if}
{#if notice}<div class="md-snackbar" role="status" aria-live="polite"><span>{notice.message}</span><button class="md-icon-button" aria-label="Dismiss message" onclick={()=>notice=null}><Icon name="close"/></button></div>{/if}

<style>
  .app-shell { height:100dvh; width:100%; display:grid; grid-template-columns:minmax(0,1fr); overflow:hidden; }
  .library-slot,.workspace-slot { min-width:0; min-height:0; overflow:hidden; }
  .workspace-slot { display:none; } .show-workspace .workspace-slot { display:block; } .show-workspace .library-slot { display:none; }
  .startup { display:flex; flex-direction:column; align-items:center; justify-content:center; gap:20px; height:100dvh; padding:32px; text-align:center; }
  .startup h1 { font-size:24px; font-weight:400; } .startup .md-progress { max-width:260px; }
  .md-snackbar span { flex:1; overflow-wrap:anywhere; } .md-snackbar button { color:inherit; }
  @media (min-width:900px) and (min-height:600px) {
    .app-shell { grid-template-columns:340px minmax(0,1fr); }
    .app-shell .library-slot,.app-shell .workspace-slot { display:block; }
    .library-slot { border-right:1px solid var(--md-sys-color-outline-variant); }
  }
</style>
