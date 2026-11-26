<script>
  import { onMount } from 'svelte';
  import { feedbackEnabled, sendFeedback } from '../api/backend.js';

  let enabled = $state(false);
  let message = $state('');
  let sending = $state(false);
  let error = $state('');
  let sent = $state(false);
  let tooLong = $derived(new TextEncoder().encode(message).length > 4096);

  onMount(() => {
    feedbackEnabled().then(value => { enabled = value; }).catch(() => {});
  });

  /** @param {SubmitEvent} event */
  async function submit(event) {
    event.preventDefault();
    if (sending || tooLong || !message.trim()) return;
    sending = true;
    error = '';
    sent = false;
    try {
      await sendFeedback(message);
      message = '';
      sent = true;
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      sending = false;
    }
  }
</script>

{#if enabled}
  <details class="feedback">
    <summary>Send feedback</summary>
    <form onsubmit={submit}>
      <label class="md-field">
        Your message
        <textarea rows="3" bind:value={message} readonly={sending} aria-invalid={tooLong}
          oninput={() => { error = ''; sent = false; }}></textarea>
      </label>
      {#if tooLong}<p class="md-error-text" role="alert">Shorten your message to 4,096 UTF-8 bytes.</p>{/if}
      {#if error}<p class="md-error-text" role="alert">{error}</p>{/if}
      <button class="md-button md-button--tonal" type="submit" disabled={sending || tooLong || !message.trim()}>
        {sending ? 'Sending…' : 'Send message'}
      </button>
      {#if sent}<p class="md-supporting-text" role="status">Feedback sent.</p>{/if}
    </form>
  </details>
{/if}

<style>
  .feedback { margin-top: 1.5rem; }
  summary { cursor: pointer; }
  form { display: grid; gap: 0.75rem; margin-top: 0.75rem; }
  button { justify-self: start; }
</style>
