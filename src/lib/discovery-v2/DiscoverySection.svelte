<script lang="ts">
  import type { Snippet } from 'svelte';
  import { onMount } from 'svelte';
  import { t } from '$lib/i18n';
  import { ArrowRight, ChevronLeft, ChevronRight } from 'lucide-svelte';

  interface Props {
    title: string;
    onSeeAll?: () => void;
    children: Snippet;
  }

  let { title, onSeeAll, children }: Props = $props();

  let scrollerEl: HTMLDivElement;
  let canScrollLeft = $state(false);
  let canScrollRight = $state(false);

  /**
   * Recompute the can-scroll booleans against the current scrollLeft. Drives
   * (a) the chevron disabled state and (b) the .scrollable-* classes that
   * apply mask-image fade gradients at the edges — the Cider-style cue that
   * there's more content beyond the visible area.
   */
  function recomputeScrollState() {
    if (!scrollerEl) return;
    const max = scrollerEl.scrollWidth - scrollerEl.clientWidth;
    canScrollLeft = scrollerEl.scrollLeft > 2;
    canScrollRight = scrollerEl.scrollLeft < max - 2;
  }

  function visibleItemCount(): number {
    if (!scrollerEl) return 1;
    const first = scrollerEl.firstElementChild as HTMLElement | null;
    if (!first) return 1;
    const cardWidth = first.getBoundingClientRect().width;
    if (cardWidth <= 0) return 1;
    return Math.max(1, Math.floor(scrollerEl.clientWidth / cardWidth));
  }

  function scrollByPage(direction: -1 | 1) {
    if (!scrollerEl) return;
    const first = scrollerEl.firstElementChild as HTMLElement | null;
    const cardWidth = first ? first.getBoundingClientRect().width : 200;
    const gap = 16;
    const delta = direction * visibleItemCount() * (cardWidth + gap);
    scrollerEl.scrollBy({ left: delta, behavior: 'smooth' });
  }

  onMount(() => {
    recomputeScrollState();
    const ro = new ResizeObserver(recomputeScrollState);
    ro.observe(scrollerEl);
    return () => ro.disconnect();
  });
</script>

<section class="section">
  <header class="head">
    <h2 class="title">{title}</h2>
    <div class="actions">
      {#if onSeeAll}
        <button class="see-all" type="button" onclick={onSeeAll}>
          {$t('discovery.seeAll')}
          <ArrowRight size={14} />
        </button>
      {/if}
      <button
        class="nav-btn"
        type="button"
        aria-label="Scroll left"
        disabled={!canScrollLeft}
        onclick={() => scrollByPage(-1)}
      >
        <ChevronLeft size={18} />
      </button>
      <button
        class="nav-btn"
        type="button"
        aria-label="Scroll right"
        disabled={!canScrollRight}
        onclick={() => scrollByPage(1)}
      >
        <ChevronRight size={18} />
      </button>
    </div>
  </header>
  <div
    class="scroller"
    class:scrollable-left={canScrollLeft}
    class:scrollable-right={canScrollRight}
    bind:this={scrollerEl}
    onscroll={recomputeScrollState}
  >
    {@render children()}
  </div>
</section>

<style>
  /* Cider pattern: native overflow-x scroll, no carousel virtualization.
     Mask-image gradients fade the edges to signal "there's more". Chevrons
     are in the header (not overlaid on cards) so paint cost stays in the
     section header rect, not on each scroll frame. */
  .section {
    margin-bottom: 32px;
    --scroll-gradient-size: 40px;
  }

  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    margin-bottom: 12px;
  }

  .title {
    font-size: 18px;
    font-weight: 600;
    color: var(--text-primary);
    margin: 0;
  }

  .actions {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .see-all {
    display: flex;
    align-items: center;
    gap: 4px;
    background: none;
    border: none;
    color: var(--text-muted);
    font-size: 13px;
    cursor: pointer;
    padding: 4px 8px;
    font-family: inherit;
    margin-right: 4px;
  }

  .nav-btn {
    width: 28px;
    height: 28px;
    border-radius: 50%;
    border: none;
    background: var(--bg-tertiary);
    color: var(--text-primary);
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    padding: 0;
  }

  .nav-btn:disabled {
    opacity: 0.4;
    cursor: default;
    color: var(--text-muted);
  }

  .scroller {
    display: flex;
    gap: 16px;
    overflow-x: auto;
    overflow-y: hidden;
    /* Hide scrollbar — chevrons + edge fades carry the affordance. */
    scrollbar-width: none;
  }

  .scroller::-webkit-scrollbar {
    display: none;
  }

  /* Mask gradients at edges. Cero coste de paint: CSS mask es GPU-implementable
     y solo cubre los primeros/últimos --scroll-gradient-size px del scroller. */
  .scroller.scrollable-left.scrollable-right {
    -webkit-mask-image: linear-gradient(
      to right,
      rgba(0, 0, 0, 0),
      rgb(0, 0, 0) var(--scroll-gradient-size),
      rgb(0, 0, 0) calc(100% - var(--scroll-gradient-size)),
      rgba(0, 0, 0, 0)
    );
    mask-image: linear-gradient(
      to right,
      rgba(0, 0, 0, 0),
      rgb(0, 0, 0) var(--scroll-gradient-size),
      rgb(0, 0, 0) calc(100% - var(--scroll-gradient-size)),
      rgba(0, 0, 0, 0)
    );
  }

  .scroller.scrollable-left:not(.scrollable-right) {
    -webkit-mask-image: linear-gradient(
      to right,
      rgba(0, 0, 0, 0),
      rgb(0, 0, 0) var(--scroll-gradient-size)
    );
    mask-image: linear-gradient(
      to right,
      rgba(0, 0, 0, 0),
      rgb(0, 0, 0) var(--scroll-gradient-size)
    );
  }

  .scroller.scrollable-right:not(.scrollable-left) {
    -webkit-mask-image: linear-gradient(
      to left,
      rgba(0, 0, 0, 0),
      rgb(0, 0, 0) var(--scroll-gradient-size)
    );
    mask-image: linear-gradient(
      to left,
      rgba(0, 0, 0, 0),
      rgb(0, 0, 0) var(--scroll-gradient-size)
    );
  }
</style>
