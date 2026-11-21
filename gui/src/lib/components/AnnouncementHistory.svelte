<script>
  import AnnouncementBody from './AnnouncementBody.svelte';
  import { listAnnouncementHistory, readPastAnnouncement } from '../domain/announcement.js';

  /** @type {{ platform: import('../api/types.js').Platform }} */
  let { platform } = $props();
  let expanded = $state(false);
  const dates = new Intl.DateTimeFormat(undefined, { dateStyle: 'medium', timeStyle: 'short' });

  /** @param {AbortSignal} signal */
  function loadHistory(signal) {
    return listAnnouncementHistory(localStorage).map(entry => ({
      ...entry,
      notice: readPastAnnouncement(entry, platform, signal, localStorage),
    }));
  }

  let entries = $state.raw(/** @type {ReturnType<typeof loadHistory>} */ ([]));
  let unavailable = $state(false);
  $effect(() => {
    if (!expanded) return;
    const controller = new AbortController();
    try {
      entries = loadHistory(controller.signal);
      unavailable = false;
    } catch {
      unavailable = true;
    }
    return () => controller.abort();
  });
</script>

<details class="history" bind:open={expanded}>
  <summary>Dismissed messages</summary>
  {#if expanded}
    {#if unavailable}
      <p class="md-supporting-text">Dismissed messages are unavailable on this device.</p>
    {:else if !entries.length}
      <p class="md-supporting-text">You haven’t dismissed any messages on this device yet.</p>
    {/if}
    {#each entries as entry (entry.timestamp + '/' + entry.file)}
      {@const date = dates.format(new Date(entry.timestamp))}
      {#await entry.notice}
        <p class="md-supporting-text" role="status">Loading message from {date}…</p>
      {:then notice}
        {#if notice}
          <details class="notice">
            <summary>{notice.title} <time datetime={entry.timestamp}>{date}</time></summary>
            <AnnouncementBody html={notice.html} />
          </details>
        {:else}
          <p class="md-supporting-text">The message from {date} is unavailable.</p>
        {/if}
      {:catch}
        <p class="md-supporting-text">The message from {date} could not be loaded.</p>
      {/await}
    {/each}
  {/if}
</details>

<style>
  summary { padding-block: 0.625rem; cursor: pointer; overflow-wrap: anywhere; }
  .history > summary { font-size: var(--md-sys-typescale-title-medium-size); font-weight: 500; }
  .notice { margin-inline-start: 1rem; }
  time { display: block; color: var(--md-sys-color-on-surface-variant); font: var(--md-sys-typescale-body-small); }
</style>
