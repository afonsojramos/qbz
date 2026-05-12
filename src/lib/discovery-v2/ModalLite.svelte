<script lang="ts">
  import { X } from 'lucide-svelte';
  import { t } from '$lib/i18n';
  import type { Snippet } from 'svelte';

  interface Props {
    isOpen: boolean;
    onClose: () => void;
    title?: string;
    showCloseButton?: boolean;
    maxWidth?: string;
    children: Snippet;
    footer?: Snippet;
  }

  let {
    isOpen,
    onClose,
    title,
    showCloseButton = true,
    maxWidth = '480px',
    children,
    footer,
  }: Props = $props();

  /**
   * Discovery V2 modal — overlay-less variant.
   *
   * Earlier attempts kept a full-viewport `rgba(0,0,0,0.7)` scrim under
   * the modal for visual focus. The shared `Modal.svelte` does the same.
   * On WebKitGTK under software compositing (any non-maximized window),
   * adding that fixed-position scrim forced an extra paint pass against
   * the underlying DiscoveryView every frame — the user reported 2-3s
   * to render even with no animation. `will-change: opacity` + GPU layer
   * hint helped some (3s → 2s) but didn't fix it.
   *
   * This variant drops the scrim entirely. The modal floats above the
   * page with a heavy border + box-shadow doing the focus work that the
   * scrim used to. Click-outside-to-close is bound on `document` so it
   * still works even without an overlay element to absorb the click.
   * ESC still closes via window keydown.
   *
   * Trade-off: less visual emphasis on the modal (background isn't
   * darkened). Gain: instant mount under software compositing because
   * there's no full-viewport rect to paint against.
   */

  let modalEl = $state<HTMLDivElement | null>(null);

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape' && isOpen) onClose();
  }

  function handleDocumentClick(e: MouseEvent) {
    if (!isOpen) return;
    if (modalEl && !modalEl.contains(e.target as Node)) onClose();
  }

  // Document-level outside-click only attaches while the modal is open.
  // The mousedown phase fires before click, so we use it to avoid the
  // race where opening the modal via a button-click also fires this
  // handler (the click event that opened the modal would already have
  // bubbled to document by the time mousedown registers next).
  $effect(() => {
    if (!isOpen) return;
    // Defer registering until the next frame so the click that opened
    // the modal doesn't immediately close it on the same event loop.
    let cancelled = false;
    requestAnimationFrame(() => {
      if (cancelled) return;
      document.addEventListener('mousedown', handleDocumentClick);
    });
    return () => {
      cancelled = true;
      document.removeEventListener('mousedown', handleDocumentClick);
    };
  });

  function portal(node: HTMLElement) {
    document.body.appendChild(node);
    return {
      destroy() {
        node.remove();
      },
    };
  }
</script>

<svelte:window onkeydown={handleKeydown} />

{#if isOpen}
  <div
    class="modal"
    use:portal
    bind:this={modalEl}
    role="dialog"
    aria-modal="true"
    tabindex="-1"
    style="max-width: {maxWidth}"
  >
    {#if title || showCloseButton}
      <div class="modal-header">
        {#if title}
          <h2>{title}</h2>
        {:else}
          <div></div>
        {/if}
        {#if showCloseButton}
          <button class="close-btn" type="button" aria-label={$t('actions.close')} onclick={onClose}>
            <X size={18} />
          </button>
        {/if}
      </div>
    {/if}
    <div class="modal-body">
      {@render children()}
    </div>
    {#if footer}
      <div class="modal-footer">
        {@render footer()}
      </div>
    {/if}
  </div>
{/if}

<style>
  /* Overlay-less floating modal. Centered via fixed positioning + 50/50
     translate. Border + box-shadow give the modal "lift" without
     darkening the rest of the viewport. No scrim element means no
     full-viewport paint pass when the modal mounts — that was the
     dominant cost on non-maximized windows. */
  .modal {
    position: fixed;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    width: calc(100% - 40px);
    max-height: calc(100dvh - 40px);
    background: var(--bg-primary);
    border: 1px solid var(--bg-tertiary);
    border-radius: 12px;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    z-index: 200000;
    /* Flat shadow stack instead of a 64px Gaussian — under software
       compositing the blur radius dominates paint cost and re-rasterizes
       whenever anything beneath the modal repaints. A composite of a
       hairline edge shadow + a small offset shadow reads as "floating"
       without the expensive blur. */
    box-shadow: 0 0 0 1px rgba(0, 0, 0, 0.6),
      0 8px 16px rgba(0, 0, 0, 0.5);
  }

  .modal-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 16px 20px;
    border-bottom: 1px solid var(--bg-tertiary);
    flex-shrink: 0;
  }

  .modal-header h2 {
    font-size: 18px;
    font-weight: 600;
    color: var(--text-primary);
    margin: 0;
  }

  .close-btn {
    width: 28px;
    height: 28px;
    border-radius: 4px;
    border: none;
    background: transparent;
    color: var(--text-muted);
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    padding: 0;
  }

  .close-btn:hover {
    background: var(--bg-tertiary);
    color: var(--text-primary);
  }

  .modal-body {
    padding: 20px;
    overflow-y: auto;
    flex: 1;
  }

  .modal-footer {
    padding: 12px 20px;
    border-top: 1px solid var(--bg-tertiary);
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    flex-shrink: 0;
  }
</style>
