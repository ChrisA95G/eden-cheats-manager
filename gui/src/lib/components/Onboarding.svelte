<script>
  import { invoke } from '@tauri-apps/api/core';
  import { onMount, untrack } from 'svelte';

  /** @type {{ currentSettings: any, ondone: function }} */
  let { currentSettings, ondone } = $props();

  // Snapshot initial prop values — untrack prevents Svelte from warning about reactive prop reads
  const _cs = /** @type {any} */ (untrack(() => currentSettings ?? {}));

  let pcLoadDir = $state(_cs.pcLoadDir ?? '');
  let apiToken = $state(_cs.apiToken ?? '');
  let detectedLoadDir = $state('');

  onMount(async () => {
    if (currentSettings?.onboardingDone) {
      ondone?.(buildResult());
    }
    try {
      detectedLoadDir = await invoke('detect_pc_load_dir') ?? '';
      if (!pcLoadDir) pcLoadDir = detectedLoadDir;
    } catch (_) {}
  });

  function buildResult() {
    return {
      ...currentSettings,
      pcLoadDir,
      apiToken,
      onboardingDone: true,
    };
  }

  let debugMsg = $state('');

  function finish() {
    const result = buildResult();
    console.log('[Onboarding] calling ondone with', result);
    debugMsg = 'Finishing setup…';
    ondone?.(result);
  }
</script>

<div class="onboarding">
  <div class="wizard-card">
    <div class="card-header">
      <div class="card-title-row">
        <span class="card-brand">ECM</span>
        <span class="card-title">EDEN CHEATS MANAGER</span>
      </div>
      <p class="step-label">// STEP 1 OF 1</p>
    </div>

    <div class="step">
      <h2>API TOKEN</h2>
      <p class="hint">
        Create a free account at <a href="https://www.cheatslips.com" target="_blank" rel="noreferrer">cheatslips.com</a> to get a token. Required to download cheat files.
      </p>

      <div class="section">
        <h3>Eden load directory</h3>
        {#if detectedLoadDir}
          <p class="hint small">Detected: <code>{detectedLoadDir}</code></p>
        {/if}
        <label>
          Path
          <input bind:value={pcLoadDir} placeholder="~/.local/share/eden/load/" />
        </label>
      </div>

      <label>
        Token <span class="optional">(can add later in Settings)</span>
        <input type="password" bind:value={apiToken} placeholder="Your cheatslips.com token" />
      </label>

      <button class="btn-primary" onclick={finish}>[ FINISH SETUP ]</button>
      {#if debugMsg}<p class="msg">{debugMsg}</p>{/if}
    </div>
  </div>
</div>

<style>
  .onboarding {
    min-height: 100vh;
    display: flex;
    align-items: flex-start;
    justify-content: center;
    background: var(--bg);
    padding: 1.5rem 1rem;
    overflow-y: auto;
  }

  .wizard-card {
    background: var(--surface);
    border: 1px solid var(--border);
    border-top: 2px solid var(--accent);
    padding: 1.5rem;
    max-width: 460px;
    width: 100%;
    margin: auto 0;
    max-height: calc(100vh - 3rem);
    overflow-y: auto;
    box-shadow: 0 0 30px rgba(0, 0, 0, 0.6), 0 0 15px rgba(245, 168, 0, 0.05);
  }

  .card-header {
    margin-bottom: 1.1rem;
    padding-bottom: .75rem;
    border-bottom: 1px solid var(--border);
  }
  .card-title-row {
    display: flex;
    align-items: baseline;
    gap: .65rem;
    margin-bottom: .2rem;
  }
  .card-brand {
    font-size: .9rem;
    letter-spacing: .25em;
    color: var(--accent);
  }
  .card-title {
    font-size: .78rem;
    letter-spacing: .12em;
    color: var(--text-muted);
  }
  .step-label { font-size: .68rem; color: var(--text-dim); margin: 0; letter-spacing: .08em; }

  h2 {
    font-size: .85rem;
    color: var(--text);
    margin: 0 0 .6rem;
    letter-spacing: .1em;
  }
  h3 {
    font-size: .62rem;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: .1em;
    margin: 0 0 .45rem;
  }

  .step { display: flex; flex-direction: column; gap: .6rem; }

  label { display: flex; flex-direction: column; gap: .2rem; font-size: .72rem; color: var(--text-muted); letter-spacing: .04em; }

  input {
    background: var(--surface2);
    border: 1px solid var(--border);
    color: var(--text);
    padding: .38rem .55rem;
    font-size: .78rem;
    outline: none;
    width: 100%;
    font-family: inherit;
    letter-spacing: .03em;
    transition: border-color .12s;
  }
  input:focus { border-color: var(--accent); box-shadow: 0 0 0 1px var(--accent-glow); }

  .btn-primary {
    background: var(--accent-dim);
    color: var(--accent);
    border: 1px solid var(--accent);
    padding: .45rem 1rem;
    font-size: .75rem;
    cursor: pointer;
    align-self: flex-start;
    font-family: inherit;
    letter-spacing: .12em;
    transition: background .15s, box-shadow .15s;
  }
  .btn-primary:not(:disabled):hover { background: rgba(245,168,0,0.15); box-shadow: 0 0 8px var(--accent-glow); }
  .btn-primary:disabled { opacity: .35; cursor: default; }

  .section {
    border: 1px solid var(--border);
    padding: .65rem .75rem;
    display: flex;
    flex-direction: column;
    gap: .45rem;
  }

  .msg {
    font-size: .72rem;
    color: var(--text-muted);
    margin: 0;
    padding: .3rem .5rem;
    background: var(--surface2);
    border-left: 2px solid var(--border);
    letter-spacing: .03em;
  }
  .hint { font-size: .72rem; color: var(--text-muted); margin: 0; line-height: 1.5; }
  .hint a { color: var(--text-muted); text-decoration: underline; }
  .hint a:hover { color: var(--accent); }
  .hint.small { font-size: .68rem; }
  .optional { font-size: .65rem; color: var(--text-dim); }

  code {
    font-family: inherit;
    font-size: .75rem;
    background: var(--surface2);
    padding: .05rem .3rem;
    border: 1px solid var(--border);
  }

</style>
