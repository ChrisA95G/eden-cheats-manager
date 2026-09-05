<script>
  import { onDestroy, onMount, tick } from 'svelte';
  import Icon from './Icon.svelte';

  /**
   * @type {{
   *   open?: boolean,
   *   title: string,
   *   children?: import('svelte').Snippet,
   *   actions?: import('svelte').Snippet,
   *   onclose?: () => void,
   * }}
   */
  let {
    open = false,
    title,
    children,
    actions,
    onclose,
  } = $props();

  const id = $props.id();
  const titleId = `${id}-title`;

  /** @type {HTMLDialogElement | null} */
  let dialog = $state(null);
  /** @type {HTMLElement | null} */
  let opener = null;

  const focusableSelector = [
    'a[href]',
    'button:not(:disabled)',
    'input:not(:disabled):not([type="hidden"])',
    'select:not(:disabled)',
    'textarea:not(:disabled)',
    '[contenteditable="true"]',
    '[tabindex]:not([tabindex="-1"])',
  ].join(',');

  /** @param {HTMLDialogElement} target */
  async function focusInitialControl(target) {
    await tick();
    if (dialog !== target || !target.open) return;

    const candidates = [
      ...target.querySelectorAll('[autofocus]:not(:disabled)'),
      ...(target.querySelector('.dialog__content')?.querySelectorAll(focusableSelector) ?? []),
      ...target.querySelectorAll(focusableSelector),
    ];
    const focusTarget = candidates.find(
      (candidate) => candidate instanceof HTMLElement && candidate.getClientRects().length > 0,
    );

    if (focusTarget instanceof HTMLElement) {
      focusTarget.focus();
    } else {
      target.focus();
    }
  }

  function restoreOpenerFocus() {
    const target = opener;
    opener = null;
    if (!target?.isConnected) return;
    queueMicrotask(() => target.focus());
  }

  function requestClose() {
    if (onclose) {
      onclose();
    } else {
      dialog?.close();
    }
  }

  /** @param {Event} event */
  function handleCancel(event) {
    event.preventDefault();
    requestClose();
  }

  /** @param {MouseEvent} event */
  function handleBackdropClick(event) {
    if (event.target !== dialog || !dialog) return;
    const rect = dialog.getBoundingClientRect();
    if (event.clientX < rect.left || event.clientX > rect.right || event.clientY < rect.top || event.clientY > rect.bottom) requestClose();
  }

  function releaseHistory() {
    if (history.state?.ecmDialog === id) history.back();
  }
  onMount(() => {
    const back = () => { if (dialog?.open && history.state?.ecmDialog !== id) requestClose(); };
    window.addEventListener('popstate', back);
    return () => window.removeEventListener('popstate', back);
  });

  $effect(() => {
    const target = dialog;
    if (!target) return;

    if (open && !target.open) {
      opener = document.activeElement instanceof HTMLElement
        ? document.activeElement
        : null;
      target.showModal();
      history.pushState({...history.state, ecmDialog: id}, '');
      void focusInitialControl(target);
    } else if (!open && target.open) {
      target.close();
      releaseHistory();
    }
  });

  onDestroy(() => {
    if (typeof history !== 'undefined' && dialog?.open) releaseHistory();
    restoreOpenerFocus();
  });
</script>

<dialog
  bind:this={dialog}
  class="dialog md-dialog"
  aria-labelledby={titleId}
  tabindex="-1"
  oncancel={handleCancel}
  onclick={handleBackdropClick}
  onclose={restoreOpenerFocus}
>
  <header class="md-dialog__header dialog__header">
    <h2 id={titleId}>{title}</h2>
    <button
      type="button"
      class="md-icon-button dialog__close"
      aria-label="Close dialog"
      onclick={requestClose}
    >
      <Icon name="close" />
    </button>
  </header>

  <div class="md-dialog__content dialog__content">
    {#if children}
      {@render children()}
    {/if}
  </div>

  {#if actions}
    <footer class="md-dialog__actions dialog__actions">
      {@render actions()}
    </footer>
  {/if}
</dialog>

<style>
  dialog.dialog {
    position: fixed;
    inset: 0;
    margin: auto;
    padding: 0;
    border: 0;
  }

  dialog.dialog:not([open]) {
    display: none;
  }

  dialog::backdrop {
    background: color-mix(in srgb, var(--md-sys-color-scrim) 32%, transparent);
  }

  .dialog__header {
    flex: 0 0 auto;
  }

  h2 {
    min-width: 0;
    flex: 1;
    color: var(--md-sys-color-on-surface);
    font-size: var(--md-sys-typescale-headline-small-size);
    font-weight: 400;
    line-height: 2rem;
  }

  .dialog__close {
    margin-inline-start: auto;
  }

  .dialog__content {
    min-height: 0;
    overscroll-behavior: contain;
  }

  .dialog__actions {
    flex: 0 0 auto;
    flex-wrap: wrap;
  }

  .dialog__actions :global(button),
  .dialog__actions :global(a) {
    min-height: var(--md-sys-size-touch);
  }

  @media (max-width: 599px), (max-height: 599px) {
    dialog.dialog {
      width: 100%;
      max-width: none;
      height: 100dvh;
      max-height: 100dvh;
      margin: 0;
      border-radius: 0;
    }

    .dialog__header {
      padding-block-start: max(0.5rem, env(safe-area-inset-top));
      padding-inline-start: max(1rem, env(safe-area-inset-left));
      padding-inline-end: max(1rem, env(safe-area-inset-right));
    }

    .dialog__content {
      padding-inline-start: max(1.5rem, env(safe-area-inset-left));
      padding-inline-end: max(1.5rem, env(safe-area-inset-right));
    }

    .dialog__actions {
      padding-block-end: max(1rem, env(safe-area-inset-bottom));
      padding-inline-start: max(1rem, env(safe-area-inset-left));
      padding-inline-end: max(1rem, env(safe-area-inset-right));
    }
  }
</style>
