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
   * Discovery V2 modal — clean-room replacement for `Modal.svelte`.
   *
   * The shared modal's slide-up `transform: translateY+scale` animation
   * is GPU-friendly at maximize (WebKitGTK promotes the modal to a GPU
   * layer) but software-rasterized at smaller windows, where the user
   * reported visible render delays. Scale animations under software comp
   * require per-pixel interpolation every frame on a ~480x600 surface.
   *
   * This variant uses opacity-only animation (cheap in either path) and
   * drops the scale + translate. Same `isOpen / onClose / title /
   * maxWidth / children / footer` Props as the shared modal so it's a
   * drop-in for callers. No backdrop-filter on the overlay (same
   * decision as the perf cleanup we applied to the shared Modal).
   */

  function handleBackdropClick(e: MouseEvent) {
    if (e.target === e.currentTarget) onClose();
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape' && isOpen) onClose();
  }

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
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_interactive_supports_focus -->
  <div
    class="modal-overlay"
    use:portal
    onclick={handleBackdropClick}
    role="dialog"
    aria-modal="true"
    tabindex="-1"
  >
    <div class="modal" style="max-width: {maxWidth}">
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
  </div>
{/if}

<style>
  /* Opacity-only fade-in. No transform animation on the modal itself —
     the shared Modal.svelte's `slide-up` keyframe uses scale + translate,
     which forces per-frame pixel interpolation under software compositing
     and made smaller-window modal opens feel slow. Opacity changes are
     cheap on either path. */
  .modal-overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.7);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 20px;
    z-index: 200000;
    animation: modal-fade-in 120ms ease-out;
  }

  @keyframes modal-fade-in {
    from { opacity: 0; }
    to { opacity: 1; }
  }

  .modal {
    background: var(--bg-primary);
    border: 1px solid var(--bg-tertiary);
    border-radius: 12px;
    width: 100%;
    max-height: calc(100dvh - 40px);
    display: flex;
    flex-direction: column;
    overflow: hidden;
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
