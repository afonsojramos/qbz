<script lang="ts">
  import { tick, untrack } from 'svelte';
  import { t } from 'svelte-i18n';
  import { prepareWithSegments, layoutWithLines, type LayoutLine } from '@chenglou/pretext';
  import type {
    LyricsFont,
    LyricsFontSize,
    LyricsDimming
  } from '$lib/stores/lyricsDisplayStore';
  import type { LyricsLine as StoreLyricsLine } from '$lib/stores/lyricsStore';

  // Accept the canonical store shape, or a minimal projection for callers
  // (e.g. unsynced lyrics surfaces) that only need text.
  type LyricsLine = StoreLyricsLine | { text: string; timeMs?: number; endMs?: number };

  interface Props {
    lines: LyricsLine[];
    activeIndex?: number;
    activeProgress?: number;
    dimInactive?: boolean;
    center?: boolean;
    compact?: boolean;
    scrollToActive?: boolean;
    immersive?: boolean;
    isSynced?: boolean;
    fontMode?: LyricsFont;
    fontSizeMode?: LyricsFontSize;
    dimmingMode?: LyricsDimming;
    activeColor?: string;
    uppercase?: boolean;
  }

  let {
    lines,
    activeIndex = -1,
    activeProgress = 0,
    dimInactive = true,
    center = false,
    compact = false,
    scrollToActive = true,
    immersive = false,
    isSynced = false,
    fontMode,
    fontSizeMode,
    dimmingMode,
    activeColor,
    uppercase = false
  }: Props = $props();

  // Sung duration for a line. Uses endMs (LRC gap marker → end of vocal)
  // when available, otherwise next line's start, otherwise a 5s default.
  function getLineDuration(index: number): number {
    if (!isSynced || index < 0 || index >= lines.length) return 3000;

    const currentLine = lines[index];
    const nextLine = lines[index + 1];

    if (!currentLine?.timeMs) return 3000;
    const bound = currentLine.endMs ?? nextLine?.timeMs;
    if (!bound) return 5000;

    const duration = bound - currentLine.timeMs;
    return Math.max(1000, Math.min(10000, duration));
  }

  // Dynamic block measurement.
  //   measuredBlockMs    — wall-clock gap between two notifications. Drives
  //                        the CSS transition duration so each block's
  //                        animation matches its real audio duration (no
  //                        pauses, no accumulated lag).
  //   measuredBlockDelta — progress increment per notification. Used as a
  //                        lookahead so the visual leads the audio by one
  //                        block — by the time the transition completes,
  //                        audio has caught up to where we transitioned
  //                        to. Net: zero visible lag, while still
  //                        correcting against actual audio on every tick.
  let measuredBlockMs = $state(175);
  let measuredBlockDelta = $state(0);
  // Lookahead is suppressed on the first notification of a new line: at
  // that point we have no measurement of this line's cadence yet, and
  // adding a stale prior-line delta would push the visual into the line
  // before it should be. Flipped on as soon as we've observed one block.
  let lookaheadEnabled = $state(false);
  let prevBlockTime = 0;
  let prevBlockProgress = 0;
  let prevBlockIndex = -1;

  // $effect.pre — runs BEFORE the DOM update for the prop change that
  // triggered it. With a plain $effect, on a line transition the derived
  // below would first paint with stale (previous-line) lookahead state,
  // and only then the effect would reset it — causing a brief flash at the
  // stale position followed by a backward transition to 0. Running pre-DOM
  // means the first paint of the new line already sees lookaheadEnabled =
  // false and the snapshot is correct.
  //
  // Reads-and-writes of the same $state (measuredBlockMs feeding its own
  // EMA, lookaheadEnabled / measuredBlockDelta written below) are wrapped
  // in untrack so the effect doesn't self-trigger on its own writes — the
  // only true dependencies are activeIndex and activeProgress.
  $effect.pre(() => {
    const idx = activeIndex;
    const progress = activeProgress;

    untrack(() => {
      if (!isSynced || idx < 0) {
        prevBlockTime = 0;
        prevBlockIndex = -1;
        lookaheadEnabled = false;
        return;
      }

      const now = performance.now();

      if (idx !== prevBlockIndex) {
        // Line transition: seed transition duration close to display
        // refresh (≥20ms). With playerStore-side extrapolation feeding
        // us per-rAF progress samples, the EMA converges to ~16ms within
        // 2-3 ticks — but the FIRST tick of a new line uses this seed,
        // and an 80ms seed would visibly lag the gradient for that
        // one frame. Clear the lookahead so the first paint of the new
        // line is exactly at the store's reported progress.
        const dur = getLineDuration(idx);
        const seedMs = Math.max(20, Math.round(dur * 0.01));
        if (measuredBlockMs !== seedMs) measuredBlockMs = seedMs;
        if (measuredBlockDelta !== 0) measuredBlockDelta = 0;
        if (lookaheadEnabled) lookaheadEnabled = false;
        prevBlockIndex = idx;
        prevBlockTime = now;
        prevBlockProgress = progress;
        return;
      }

      if (prevBlockTime > 0) {
        const dt = now - prevBlockTime;
        const dp = progress - prevBlockProgress;
        // Backward progress within the same line = seek; flip lookahead off
        // so the cut doesn't lead from a now-stale forward velocity.
        if (dp < 0 && lookaheadEnabled) lookaheadEnabled = false;
        // dt > 1500 = discontinuity (tab resume, pause+seek): keep the EMA
        // unchanged AND reset the snapshot so the next tick measures from
        // the post-discontinuity moment instead of absorbing a frame-sized
        // dp built up over seconds.
        // 5ms ≤ dt ≤ 1500: a real rAF-rate sample, update the EMA. The 5ms
        // floor used to be 50ms (calibrated for setInterval) which silently
        // rejected every rAF update.
        if (dt >= 5 && dt <= 1500) {
          const newMs = Math.round(measuredBlockMs * 0.3 + dt * 0.7);
          if (newMs !== measuredBlockMs) measuredBlockMs = newMs;
        }
        // Forward-only progress deltas; clamp to 1/30 (~half a rAF tick at
        // 60Hz on a slow line) so a single coarse audio-time jump can't
        // poison the lookahead with a multi-frame lead.
        if (dp > 0 && dp < 0.2) {
          const clamped = Math.min(dp, 1 / 30);
          measuredBlockDelta = measuredBlockDelta * 0.3 + clamped * 0.7;
          if (!lookaheadEnabled) lookaheadEnabled = true;
        }
      }
      prevBlockTime = now;
      prevBlockProgress = progress;
    });
  });

  // Active lyric layout via Pretext. For wrapped lyrics, the CSS gradient
  // applied to a single span shares the same X cut across every visual
  // line — visually wrong because line 2 hasn't been sung yet. Pretext
  // gives us each visual line's text and width as a clean data structure;
  // we then render each line as its own block-level <span> with its own
  // simple 0→100% gradient. Each line owns a proportional share of the
  // total karaoke progress, so line 1 fully fills before line 2 starts.
  let activeLineSegments = $state<LayoutLine[]>([]);

  function layoutActiveLine(): void {
    if (!container || activeIndex < 0 || !isSynced) {
      activeLineSegments = [];
      return;
    }
    const text = lines[activeIndex]?.text;
    if (!text) {
      activeLineSegments = [];
      return;
    }
    const div = container.querySelector<HTMLElement>(
      `[data-line-index="${activeIndex}"]`
    );
    if (!div) {
      activeLineSegments = [];
      return;
    }
    const computed = getComputedStyle(div);
    const fontSizePx = Number.parseFloat(computed.fontSize);
    // line-height of `normal` resolves to "normal" (not a px string) → NaN.
    // Anything else resolves to a positive px value; guard explicitly rather
    // than relying on `|| fallback` (which would also swallow a legitimate 0).
    const parsedLineHeight = Number.parseFloat(computed.lineHeight);
    const lineHeightPx =
      Number.isFinite(parsedLineHeight) && parsedLineHeight > 0
        ? parsedLineHeight
        : fontSizePx * 1.5;
    // offsetWidth is the CSS layout width (pre-transform). The active line
    // applies a CSS scale, so the visual rendering is wider than its
    // layout box and would clip against the parent's overflow-x: hidden.
    // 1.2 is empirical — it accounts for the visual scale plus a small
    // safety margin for measurement variance between Pretext's canvas
    // metrics and the browser's actual text layout.
    const ACTIVE_SCALE = 1.2;
    const maxWidth = div.offsetWidth / ACTIVE_SCALE;
    if (!fontSizePx || maxWidth <= 0) {
      activeLineSegments = [];
      return;
    }
    // Canvas-style font string: "<weight> <size>px <family>"
    const fontStr = `${computed.fontWeight} ${fontSizePx}px ${computed.fontFamily}`;
    // Letter-spacing in CSS gets resolved to a pixel value (or "normal").
    // Without it, Pretext underestimates width by ~spacing × char-count.
    const lsStr = computed.letterSpacing;
    const letterSpacing = lsStr && lsStr !== 'normal' ? Number.parseFloat(lsStr) || 0 : 0;

    try {
      const prepared = prepareWithSegments(text, fontStr, { letterSpacing });
      const result = layoutWithLines(prepared, maxWidth, lineHeightPx);
      activeLineSegments = result.lines;
    } catch (e) {
      if (import.meta.env.DEV) {
        console.warn('[Lyrics] Pretext layout failed:', e);
      }
      activeLineSegments = [];
    }
  }

  // Schedule layout after Svelte's DOM flush, but cancel any in-flight
  // schedule if activeIndex moves before the microtask resolves. Without
  // the generation guard, rapid line transitions (seek-scrub, fast songs
  // at 60Hz notifications) queue multiple `tick().then(layoutActiveLine)`
  // promises; they resolve in order against the *current* activeIndex,
  // measuring and re-measuring the same line and thrashing segment DOM.
  $effect(() => {
    const idx = activeIndex;
    if (!isSynced || idx < 0) {
      activeLineSegments = [];
      return;
    }
    let canceled = false;
    tick().then(() => {
      if (!canceled && idx === activeIndex) layoutActiveLine();
    });
    return () => { canceled = true; };
  });

  // Re-layout on container resize (sidebar toggle, window resize, font
  // size change). Coalesce bursts via rAF: a continuous window drag fires
  // RO 60+ times/sec, and every callback would otherwise force layout +
  // run Pretext measurement. Also gate on actual width change — the
  // active-line scale animation (font-size, transform) changes container
  // *height* mid-transition, which we don't care about.
  $effect(() => {
    if (!container) return;
    let pending = false;
    let prevWidth = container.clientWidth;
    const ro = new ResizeObserver((entries) => {
      const w = entries[0]?.contentRect.width ?? prevWidth;
      if (Math.abs(w - prevWidth) < 1) return;
      prevWidth = w;
      if (pending) return;
      pending = true;
      requestAnimationFrame(() => {
        pending = false;
        layoutActiveLine();
      });
    });
    ro.observe(container);
    return () => ro.disconnect();
  });

  // Effective karaoke progress (with lookahead) — always uses
  // activeProgress as a floor. Earlier the lookaheadEnabled flag gated
  // this entirely; if it ever failed to flip on (e.g. the line gets only a
  // single notification before audio stops, or a seek lands directly at
  // p=1) the visual would stay at 0. Lookahead is a bonus, not a gate.
  const effectiveProgress = $derived.by(() => {
    const base = Math.max(0, Math.min(1, activeProgress));
    return lookaheadEnabled
      ? Math.max(0, Math.min(1, base + measuredBlockDelta))
      : base;
  });

  // Pre-compute cumulative-width prefix for the segments, so per-segment
  // progress is O(1) lookup instead of O(i) summation inside the each-block.
  // segmentBounds[i] = [start, end] in pixels for segment i across the
  // full line; segmentTotal = total ink width.
  const segmentBounds = $derived.by(() => {
    const bounds: Array<[number, number]> = [];
    let cumulative = 0;
    for (const seg of activeLineSegments) {
      bounds.push([cumulative, cumulative + seg.width]);
      cumulative += seg.width;
    }
    return bounds;
  });
  const segmentTotalWidth = $derived(
    segmentBounds.length > 0 ? segmentBounds[segmentBounds.length - 1][1] : 0
  );

  // Compute a single segment's local progress (0→1) given the global
  // effectiveProgress and that segment's share of total width. Line 1
  // fills first, line 2 only starts once line 1 is fully sung.
  function getSegmentProgress(i: number): number {
    if (segmentTotalWidth <= 0) return 0;
    const progressPx = effectiveProgress * segmentTotalWidth;
    const [start, end] = segmentBounds[i];
    const width = end - start;
    if (width <= 0) return 0;
    const localPx = Math.max(0, Math.min(width, progressPx - start));
    return localPx / width;
  }

  // Inline style for the active line's parent div. Now only carries
  // --block-interval (shared transition timing for all segments) — the
  // per-segment --line-progress is set on each segment span itself.
  const activeLineStyle = $derived.by(() => {
    if (!isSynced || activeIndex < 0) return '';
    return `--block-interval: ${measuredBlockMs}ms`;
  });

  let container: HTMLDivElement | null = null;
  let lastScrolledIndex = -1;
  let lastLyricsKey = '';

  // In immersive mode, use CSS-only opacity via data attributes (no inline styles)
  // This avoids per-line style recalculation on every render
  function getDistanceClass(index: number, active: number): string {
    if (!dimInactive || active < 0) return '';
    if (index === active) return '';
    const distance = Math.abs(index - active);
    if (distance === 1) return 'distance-1';
    if (distance === 2) return 'distance-2';
    if (distance === 3) return 'distance-3';
    return 'distance-far';
  }

  // Only calculate inline opacity for non-immersive mode (karaoke needs precise values)
  function getLineOpacity(index: number, active: number): number {
    if (!dimInactive || active < 0) return 1;
    if (index === active) return 1;

    // Sidebar dimming override (only when dimmingMode is provided)
    if (dimmingMode === 'off') return 1;
    if (dimmingMode === 'soft') return 0.6;
    // 'strong' (or undefined) falls through to the existing ladder

    const distance = Math.abs(index - active);
    if (distance === 1) return 0.5;
    if (distance === 2) return 0.35;
    if (distance === 3) return 0.25;
    return 0.15;
  }

  // Scroll active line into view (centered)
  // instant: true for catch-up sync, false for normal progression
  async function scrollActiveIntoView(index: number, instant: boolean = false) {
    if (!container || index < 0) return;

    await tick();

    const target = container.querySelector<HTMLElement>(`[data-line-index="${index}"]`);
    if (!target) return;

    const containerRect = container.getBoundingClientRect();
    const targetRect = target.getBoundingClientRect();
    const targetCenter = targetRect.top + targetRect.height / 2;
    const containerCenter = containerRect.top + containerRect.height / 2;
    const scrollOffset = targetCenter - containerCenter;

    container.scrollBy({
      top: scrollOffset,
      behavior: instant ? 'instant' : 'smooth'
    });
  }

  // React to activeIndex changes - scroll to keep active line visible
  $effect(() => {
    if (!scrollToActive || activeIndex < 0 || !isSynced) return;
    if (activeIndex === lastScrolledIndex) return;

    // Determine scroll behavior
    const isLargeJump = lastScrolledIndex >= 0 && Math.abs(activeIndex - lastScrolledIndex) > 2;
    const isInitialSync = lastScrolledIndex === -1 && activeIndex > 0;
    const useInstant = isLargeJump || isInitialSync;

    lastScrolledIndex = activeIndex;
    scrollActiveIntoView(activeIndex, useInstant);
  });

  // Reset scroll tracking when lyrics change (new track)
  // Use first line text as key to detect actual content change, not just array reference
  $effect(() => {
    const newKey = lines.length > 0 ? `${lines.length}-${lines[0].text}` : '';
    if (newKey !== lastLyricsKey) {
      lastLyricsKey = newKey;
      lastScrolledIndex = -1;
    }
  });
</script>

<div
  class="lyrics-lines"
  class:compact
  class:center
  class:immersive
  class:static={!isSynced}
  class:size-small={compact && fontSizeMode === 'small'}
  class:size-medium={compact && fontSizeMode === 'medium'}
  class:size-large={compact && fontSizeMode === 'large'}
  class:size-xl={compact && fontSizeMode === 'xl'}
  class:font-line-seed-jp={compact && fontMode === 'line-seed-jp'}
  class:font-montserrat={compact && fontMode === 'montserrat'}
  class:font-noto-sans={compact && fontMode === 'noto-sans'}
  class:font-source-sans-3={compact && fontMode === 'source-sans-3'}
  class:uppercase={compact && uppercase}
  style:--lyrics-active-color={compact && activeColor ? activeColor : null}
  bind:this={container}
>
  {#if lines.length === 0}
    <div class="lyrics-empty">{$t('player.noLyrics')}</div>
  {:else}
    <!-- Spacer at top to allow first lines to scroll to center (only for synced) -->
    {#if isSynced}
      <div class="lyrics-spacer"></div>
    {/if}

    {#each lines as line, index (index)}
      <!-- Single <div> per line with class toggles, so CSS transitions can
           animate state changes (active ↔ past, size/scale/color) instead
           of being killed by destroy-and-recreate when activeIndex moves.
           Active line renders one .line-segment per Pretext-laid-out
           visual line so each gets its own gradient and sequential fill;
           non-active lines render a single .line-text. -->
      {@const isActive = isSynced && index === activeIndex}
      <div
        class="lyrics-line {immersive && isSynced ? getDistanceClass(index, activeIndex) : ''}"
        class:active={isActive}
        class:past={isSynced && index < activeIndex}
        style={isActive
          ? activeLineStyle
          : (immersive ? '' : `--line-opacity: ${isSynced ? getLineOpacity(index, activeIndex) : 1}`)}
        data-line-index={index}
      >
        {#if isActive && activeLineSegments.length > 0}
          <!-- Key includes activeIndex so a line transition forces fresh
               DOM for each segment. Otherwise keying by `i` alone reuses
               the previous line's segment-i element, and CSS transitions
               on --line-progress fire from the prior line's final value
               (e.g. 1.0 if that line was fully sung) down to the new
               line's value — visible as a "ghost" partial-sung start
               on the new line's segments. -->
          {#each activeLineSegments as segment, i (`${activeIndex}-${i}`)}
            <span
              class="line-segment"
              style="--line-progress: {getSegmentProgress(i)}"
            >{segment.text}</span>
          {/each}
        {:else}
          <span class="line-text">{line.text}</span>
        {/if}
      </div>
    {/each}

    <!-- Spacer at bottom to allow last lines to scroll to center (only for synced) -->
    {#if isSynced}
      <div class="lyrics-spacer"></div>
    {/if}
  {/if}
</div>

<style>
  .lyrics-lines {
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding: 16px 20px;
    overflow-y: auto;
    overflow-x: hidden;
    height: 100%;
    scrollbar-width: thin;
    scrollbar-color: var(--bg-tertiary) transparent;
  }

  .lyrics-lines::-webkit-scrollbar {
    width: 6px;
  }

  .lyrics-lines::-webkit-scrollbar-track {
    background: transparent;
  }

  .lyrics-lines::-webkit-scrollbar-thumb {
    background: var(--bg-tertiary);
    border-radius: 3px;
  }

  /* Immersive mode: hide scrollbar but keep scrolling */
  .lyrics-lines.immersive {
    scrollbar-width: none;
  }

  .lyrics-lines.immersive::-webkit-scrollbar {
    display: none;
  }

  .lyrics-spacer {
    min-height: 40vh;
    flex-shrink: 0;
  }

  /* Static mode - non-synced lyrics, start at top */
  .lyrics-lines.static {
    justify-content: flex-start;
  }

  .lyrics-lines.static .lyrics-line {
    opacity: 0.85;
    color: var(--text-primary);
  }

  .lyrics-lines.center {
    text-align: center;
  }

  .lyrics-lines.compact {
    gap: 12px;
  }

  .lyrics-lines.compact .lyrics-line {
    font-size: 15px;
  }

  .lyrics-lines.compact .lyrics-line.active {
    font-size: 17px;
  }

  /* Immersive mode - larger text with Oswald font */
  /* Performance: uses CSS classes for opacity instead of inline styles */
  .lyrics-lines.immersive {
    gap: clamp(18px, 2.5vh, 30px);
    padding: 16px 24px;
    /* Containment: isolate layout/paint to this subtree */
    contain: layout style;
  }

  .lyrics-lines.immersive .lyrics-line {
    font-family: 'Montserrat', var(--font-sans), sans-serif;
    font-size: clamp(24px, 2.6vw, 34px);
    font-weight: 500;
    line-height: 1.35;
    letter-spacing: 0.01em;
    /* Text shadow for contrast against any background */
    text-shadow:
      0 1px 2px rgba(0, 0, 0, 0.5),
      0 2px 8px rgba(0, 0, 0, 0.3);
    /* Remove expensive transitions in immersive mode */
    transition: opacity 200ms ease-out, color 200ms ease-out;
    /* Containment per line */
    contain: layout style;
  }

  /* Distance-based opacity classes (CSS-only, no inline styles) */
  .lyrics-lines.immersive .lyrics-line.distance-1 {
    opacity: 0.5;
  }
  .lyrics-lines.immersive .lyrics-line.distance-2 {
    opacity: 0.35;
  }
  .lyrics-lines.immersive .lyrics-line.distance-3 {
    opacity: 0.25;
  }
  .lyrics-lines.immersive .lyrics-line.distance-far {
    opacity: 0.15;
  }

  .lyrics-lines.immersive .lyrics-line.active {
    font-size: clamp(28px, 3.2vw, 42px);
    font-weight: 700;
    color: #ffffff !important;
    opacity: 1;
  }

  /* Immersive mode: simple bright white text.
     Also overrides the sidebar's karaoke gradient + transparent fill so the
     active line stays solid white instead of gradient-clipped. */
  .lyrics-lines.immersive .lyrics-line.active .line-text {
    background: none;
    color: #ffffff !important;
    -webkit-text-fill-color: #ffffff;
  }

  /* Past lines in immersive should be clearly dimmer than active */
  .lyrics-lines.immersive .lyrics-line.past {
    color: rgba(255, 255, 255, 0.35);
    font-weight: 400;
  }

  .lyrics-line {
    color: var(--text-secondary);
    font-family: var(--font-sans);
    font-size: 16px;
    font-weight: 500;
    line-height: 1.5;
    letter-spacing: 0.01em;
    opacity: var(--line-opacity, 1);
    /* Transitions on every property the active class swaps, so going
       active ↔ past animates smoothly in both directions (the same DOM
       element persists across state changes — see the each-block above). */
    transition:
      opacity 220ms ease-out,
      color 220ms ease-out,
      font-size 220ms cubic-bezier(0.4, 0, 0.2, 1),
      font-weight 220ms ease-out,
      transform 220ms cubic-bezier(0.4, 0, 0.2, 1);
    transform-origin: left center;
    /* Prevent horizontal overflow with long lyrics */
    word-wrap: break-word;
    overflow-wrap: break-word;
  }

  /* Register --line-progress as an animatable number so CSS can interpolate
     the gradient stop position between block notifications. */
  @property --line-progress {
    syntax: '<number>';
    inherits: true;
    initial-value: 0;
  }

  /* Active line: mirror the base transitions and add --line-progress
     (interpolated over --block-interval, set per-line in JS to match audio
     speed). Re-declaring the full list because `transition` is shorthand
     and would otherwise drop the size/scale/color animations on activation. */
  .lyrics-line.active {
    transition:
      opacity 220ms ease-out,
      color 220ms ease-out,
      font-size 220ms cubic-bezier(0.4, 0, 0.2, 1),
      font-weight 220ms ease-out,
      transform 220ms cubic-bezier(0.4, 0, 0.2, 1),
      --line-progress var(--block-interval, 175ms) linear;
  }

  /* CPU mode: the blanket `transition: none !important` in
     ImmersivePlayer nukes the line-change transitions when these lyrics
     are rendered in immersive split mode. Restore the cheap ones —
     opacity, color and transform are compositor-only (basically free
     under software compositing). We deliberately skip `font-size`
     because that one forces a full re-layout per animation frame, which
     IS expensive in CPU mode. The `!important` is required to beat the
     equally-important blanket rule. */
  :global(html.no-hwaccel .immersive-player .lyrics-line) {
    transition:
      opacity 200ms ease-out,
      color 200ms ease-out !important;
  }

  :global(html.no-hwaccel .immersive-player .lyrics-line.active) {
    transition:
      opacity 200ms ease-out,
      transform 200ms ease-out,
      color 250ms ease-out !important;
  }

  .lyrics-lines.center .lyrics-line {
    transform-origin: center center;
  }

  .lyrics-line.past {
    color: var(--text-muted);
  }

  .lyrics-line.active {
    color: var(--text-primary);
    font-size: 20px;
    font-weight: 700;
    opacity: 1;
    transform: scale(1.02);
    /* No text-shadow or filter on the active line — text-shadow inherits
       into the background-clipped span and tints the gradient on WebKit
       (macOS), while filter: drop-shadow rasterizes the line at its
       layout box and clips descenders (g, y, p). Bold + scale + colored
       gradient is enough emphasis without either. */
  }

  .lyrics-lines.center .lyrics-line.active {
    transform: scale(1.05);
  }

  /* Make .line-text block-level so background-clip: text has a paint area
     bounded by the full block box rather than the inline-fragment box.
     On WebKit the inline-fragment paint area can be tight around the
     font's ascent/descent metrics, so glyph parts that extend below
     baseline (g, y, p, q, j descenders) end up outside the paint area
     and render transparent against the parent background. A small
     padding-bottom gives descenders a guaranteed paint margin. */
  .lyrics-line .line-text {
    display: block;
    padding-bottom: 0.15em;
  }

  /* One .line-segment per Pretext-laid-out visual line of the active
     lyric. Each segment is its own block-level element with its own
     simple 0→100% gradient driven by --line-progress (set inline per
     segment in JS based on its share of total progress). Sequential
     filling falls out naturally: line 1's segment owns the first portion
     of progress, line 2's the next, etc.
     white-space: nowrap because Pretext has already done the wrapping;
     CSS shouldn't re-wrap within a segment. padding-bottom gives the
     background-clip: text mask room for descenders (g/y/p/q). */
  .lyrics-line.active .line-segment {
    display: block;
    white-space: nowrap;
    padding-bottom: 0.15em;
    --progress-pos: calc(var(--line-progress, 0) * 100%);
    background: linear-gradient(
      90deg,
      var(--lyrics-active-color, var(--accent-primary)) 0%,
      var(--lyrics-active-color, var(--accent-primary)) var(--progress-pos),
      var(--text-primary) var(--progress-pos),
      var(--text-primary) 100%
    );
    -webkit-background-clip: text;
    background-clip: text;
    color: transparent;
    -webkit-text-fill-color: transparent;
    transition: --line-progress var(--block-interval, 175ms) linear;
  }

  /* Immersive mode overrides the gradient so the active line is solid white. */
  .lyrics-lines.immersive .lyrics-line.active .line-segment {
    background: none;
    color: #ffffff !important;
    -webkit-text-fill-color: #ffffff;
  }

  /* CPU mode: kill the karaoke gradient in non-immersive lyrics views too
     (sidebar/static/compact). background-clip: text repaint + per-frame
     --line-progress interpolation are both expensive under software
     compositing. Drop to a solid accent color; emphasis still reads via
     font-size + weight + scale from .lyrics-line.active. Immersive
     already neutralizes the gradient above (.lyrics-lines.immersive
     specificity 0,0,5,0 wins over our 0,0,4,1), so this rule only fires
     in the non-immersive surfaces. */
  :global(html.no-hwaccel .lyrics-line.active .line-segment) {
    background: none !important;
    color: var(--lyrics-active-color, var(--accent-primary)) !important;
    -webkit-text-fill-color: var(--lyrics-active-color, var(--accent-primary)) !important;
    transition: none !important;
  }

  @media (prefers-reduced-motion: reduce) {
    .lyrics-line.active {
      transition: none;
    }
  }

  .lyrics-empty {
    color: var(--text-muted);
    font-size: 14px;
    text-align: center;
    padding: 48px 0;
  }

  /* Sidebar font size overrides (compact mode only) */
  .lyrics-lines.compact.size-small .lyrics-line {
    font-size: 13px;
  }
  .lyrics-lines.compact.size-small .lyrics-line.active {
    font-size: 15px;
  }
  .lyrics-lines.compact.size-medium .lyrics-line {
    font-size: 15px;
  }
  .lyrics-lines.compact.size-medium .lyrics-line.active {
    font-size: 17px;
  }
  .lyrics-lines.compact.size-large .lyrics-line {
    font-size: 18px;
  }
  .lyrics-lines.compact.size-large .lyrics-line.active {
    font-size: 21px;
  }
  .lyrics-lines.compact.size-xl .lyrics-line {
    font-size: 22px;
  }
  .lyrics-lines.compact.size-xl .lyrics-line.active {
    font-size: 26px;
  }

  /* Sidebar uppercase override (compact mode only) */
  .lyrics-lines.compact.uppercase .lyrics-line {
    text-transform: uppercase;
  }

  /* Sidebar font family overrides (compact mode only) */
  .lyrics-lines.compact.font-line-seed-jp .lyrics-line {
    font-family: 'LINE Seed JP', -apple-system, BlinkMacSystemFont, 'Segoe UI', system-ui, sans-serif;
  }
  .lyrics-lines.compact.font-montserrat .lyrics-line {
    font-family: 'Montserrat', -apple-system, BlinkMacSystemFont, 'Segoe UI', system-ui, sans-serif;
  }
  .lyrics-lines.compact.font-noto-sans .lyrics-line {
    font-family: 'Noto Sans', -apple-system, BlinkMacSystemFont, 'Segoe UI', system-ui, sans-serif;
  }
  .lyrics-lines.compact.font-source-sans-3 .lyrics-line {
    font-family: 'Source Sans 3', -apple-system, BlinkMacSystemFont, 'Segoe UI', system-ui, sans-serif;
  }
</style>
