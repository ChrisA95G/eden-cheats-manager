<script>
  import { onMount } from 'svelte';
  import Dialog from './ui/Dialog.svelte';
  import { fetchAnnouncement, rememberDismissal } from '../domain/announcement.js';

  /** @type {{ platform: import('../api/types.js').Platform, onupdatesettings: () => void, ondismiss: () => void }} */
  let { platform, onupdatesettings, ondismiss } = $props();
  let announcement = $state(/** @type {Awaited<ReturnType<typeof fetchAnnouncement>>} */ (null));
  let settingsRequested = false;

  onMount(() => {
    const controller = new AbortController();
    fetchAnnouncement(platform, controller.signal)
      .then(result => {
        // A slow request shouldn't interrupt a dialog the user has already opened.
        if (!controller.signal.aborted && !document.querySelector('dialog[open]')) announcement = result;
      })
      .catch(() => { /* An unreadable index or unavailable storage should not interrupt startup. */ });
    return () => controller.abort();
  });

  function dismiss() {
    if (!announcement) return;
    try { rememberDismissal(announcement, localStorage); }
    catch { /* The dialog can still be dismissed for this session. */ }
    announcement = null;
    ondismiss();
    if (settingsRequested) onupdatesettings();
  }

  function viewUpdateSettings() {
    if (settingsRequested) return;
    settingsRequested = true;
    // Let Dialog dismiss on Back before Settings opens its own history entry.
    history.back();
  }
</script>

<Dialog open={!!announcement} title={announcement?.title ?? ''} onclose={dismiss}>
  {#if announcement}
    <div class="announcement">{@html announcement.html}</div>
  {/if}
  {#snippet actions()}
    {#if announcement?.opensUpdateSettings}
      <button class="md-button md-button--text" type="button" onclick={viewUpdateSettings}>Open settings</button>
    {/if}
    <button class="md-button md-button--filled" type="button" onclick={dismiss}>Got it</button>
  {/snippet}
</Dialog>

<style>
  .announcement { overflow-wrap: anywhere; }
  .announcement :global(> * + *) { margin-top: 0.75rem; }
  .announcement :global(ul), .announcement :global(ol) { padding-inline-start: 1.5rem; }
</style>
