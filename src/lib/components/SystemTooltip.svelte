<script lang="ts">
  import { onMount, onDestroy } from 'svelte';

  let tooltipEl: HTMLDivElement;
  let visible = $state(false);
  let positioned = $state(false);
  let text = $state('');
  let position = $state({ x: 0, y: 0 });

  let hoverTimer: ReturnType<typeof setTimeout> | null = null;
  let currentTarget: HTMLElement | null = null;

  const DELAY = 400;
  const OFFSET = 8;

  onMount(() => {
    document.addEventListener('pointerenter', handleEnter, true);
    document.addEventListener('pointerleave', handleLeave, true);
    document.addEventListener('scroll', hide, true);
    window.addEventListener('blur', hide);
  });

  onDestroy(() => {
    document.removeEventListener('pointerenter', handleEnter, true);
    document.removeEventListener('pointerleave', handleLeave, true);
    document.removeEventListener('scroll', hide, true);
    window.removeEventListener('blur', hide);
    if (hoverTimer) clearTimeout(hoverTimer);
  });

  function handleEnter(e: PointerEvent) {
    const target = e.target as HTMLElement;
    if (!target?.closest) return;

    // Match elements with title (first hover) or data-sys-tooltip (subsequent hovers)
    const el = (target.closest('[title]') || target.closest('[data-sys-tooltip]')) as HTMLElement | null;
    if (!el) return;

    // On first hover, migrate title → data-sys-tooltip
    const titleText = el.getAttribute('title');
    if (titleText) {
      el.setAttribute('data-sys-tooltip', titleText);
      el.removeAttribute('title');
    }

    const storedText = el.getAttribute('data-sys-tooltip');
    if (!storedText) return;

    if (hoverTimer) clearTimeout(hoverTimer);
    currentTarget = el;

    hoverTimer = setTimeout(() => {
      if (currentTarget !== el) return;
      text = storedText;
      // Set visible but not yet positioned — tooltip renders offscreen for measurement
      positioned = false;
      visible = true;

      requestAnimationFrame(() => {
        if (!tooltipEl || currentTarget !== el) return;
        computePosition(el);
        positioned = true;
      });
    }, DELAY);
  }

  function handleLeave(e: PointerEvent) {
    const el = (e.target as HTMLElement)?.closest?.('[data-sys-tooltip]');
    if (!el || el !== currentTarget) return;
    hide();
  }

  function hide() {
    if (hoverTimer) clearTimeout(hoverTimer);
    hoverTimer = null;
    currentTarget = null;
    visible = false;
    positioned = false;
  }

  function computePosition(el: HTMLElement) {
    const rect = el.getBoundingClientRect();
    const vw = window.innerWidth;
    const vh = window.innerHeight;
    const tw = tooltipEl.offsetWidth;
    const th = tooltipEl.offsetHeight;

    let x = rect.left + rect.width / 2 - tw / 2;
    let y = rect.top - th - OFFSET;

    // If clips top, try bottom
    if (y < 4) {
      y = rect.bottom + OFFSET;
      // If clips bottom too, try right
      if (y + th > vh - 4) {
        x = rect.right + OFFSET;
        y = rect.top + rect.height / 2 - th / 2;
        // If clips right, try left
        if (x + tw > vw - 4) {
          x = rect.left - tw - OFFSET;
        }
      }
    }

    // Clamp to viewport
    x = Math.max(4, Math.min(x, vw - tw - 4));
    y = Math.max(4, Math.min(y, vh - th - 4));

    position = { x, y };
  }
</script>

<div
  class="sys-tooltip"
  class:visible={visible && positioned}
  class:measuring={visible && !positioned}
  bind:this={tooltipEl}
  style="left: {position.x}px; top: {position.y}px;"
>
  {text}
</div>

<style>
  .sys-tooltip {
    position: fixed;
    z-index: 10001;
    background: var(--bg-primary);
    border: 1px solid var(--bg-tertiary);
    border-radius: 6px;
    padding: 6px 10px;
    font-size: 12px;
    color: var(--text-secondary);
    white-space: normal;
    max-width: 360px;
    pointer-events: none;
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.3);
    opacity: 0;
    visibility: hidden;
  }

  .sys-tooltip.measuring {
    visibility: hidden;
    opacity: 0;
  }

  .sys-tooltip.visible {
    visibility: visible;
    animation: sys-tooltip-in 150ms ease forwards;
  }

  @keyframes sys-tooltip-in {
    from {
      opacity: 0;
      transform: translateY(2px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }
</style>
