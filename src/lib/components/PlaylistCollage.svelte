<script lang="ts">
  import { Music } from 'lucide-svelte';
  import { cachedSrc } from '$lib/actions/cachedImage';

  interface Props {
    artworks: string[];
    size?: number;
    class?: string;
  }

  let {
    artworks = [],
    size = 120,
    class: className = ''
  }: Props = $props();

  // Pick a Qobuz thumbnail size that fits the rendered collage. The old
  // implementation always upscaled to _600.jpg which wasted decoder budget
  // when 20+ collages of size 140 render simultaneously in the PlaylistManager
  // grid. Choose based on the individual TILE size (quad layout tiles are a
  // third of the full collage side).
  function pickQobuzSize(targetPx: number): number {
    if (targetPx <= 80) return 50;
    if (targetPx <= 200) return 150;
    if (targetPx <= 400) return 300;
    return 600;
  }
  function resizeImageUrl(url: string, targetPx: number): string {
    if (!url) return url;
    const target = pickQobuzSize(targetPx);
    const tag = `/${target}x${target}/`;
    return url
      .replace(/_(50|100|150|230|300|600|max|org)\.jpg/i, `_${target}.jpg`)
      .replace(/\/(50x50|100x100|150x150|230x230|300x300|600x600)\//, tag);
  }

  // For quad layouts the big tile is ~(size * 2/3); small tiles are ~(size / 3).
  // We use one URL size per collage as an approximation — good enough for a
  // 140px-or-smaller collage, and the cachedSrc layer dedupes requests across
  // tiles anyway so the worst case is one decoded image per unique artwork.
  const thumbSize = $derived(Math.max(40, Math.round(size * 0.66)));

  // Get unique artworks (dedupe same album covers) and size-match for the
  // collage tile they'll live in.
  const uniqueArtworks = $derived.by(() => {
    const seen = new Set<string>();
    return artworks.filter(art => {
      if (!art || seen.has(art)) return false;
      seen.add(art);
      return true;
    }).slice(0, 4).map((url) => resizeImageUrl(url, thumbSize));
  });

  const count = $derived(uniqueArtworks.length);
</script>

<div
  class="collage {className}"
  class:single={count === 1}
  class:dual={count === 2}
  class:triple={count === 3}
  class:quad={count >= 4}
  style="--size: {size}px"
>
  {#if count === 0}
    <div class="placeholder">
      <Music size={size * 0.3} />
    </div>
  {:else if count === 1}
    <img use:cachedSrc={uniqueArtworks[0]} alt="" class="cover full" loading="lazy" decoding="async" />
  {:else if count === 2}
    <img use:cachedSrc={uniqueArtworks[0]} alt="" class="cover half-left" loading="lazy" decoding="async" />
    <img use:cachedSrc={uniqueArtworks[1]} alt="" class="cover half-right" loading="lazy" decoding="async" />
  {:else if count === 3}
    <img use:cachedSrc={uniqueArtworks[0]} alt="" class="cover half-left" loading="lazy" decoding="async" />
    <img use:cachedSrc={uniqueArtworks[1]} alt="" class="cover quarter top-right" loading="lazy" decoding="async" />
    <img use:cachedSrc={uniqueArtworks[2]} alt="" class="cover quarter bottom-right" loading="lazy" decoding="async" />
  {:else}
    <!-- 4 covers: 3 small stacked left, 1 large right -->
    <img use:cachedSrc={uniqueArtworks[0]} alt="" class="cover small-top" loading="lazy" decoding="async" />
    <img use:cachedSrc={uniqueArtworks[1]} alt="" class="cover small-mid" loading="lazy" decoding="async" />
    <img use:cachedSrc={uniqueArtworks[2]} alt="" class="cover small-bot" loading="lazy" decoding="async" />
    <img use:cachedSrc={uniqueArtworks[3]} alt="" class="cover large-right" loading="lazy" decoding="async" />
  {/if}
</div>

<style>
  .collage {
    position: relative;
    width: var(--size, 120px);
    height: var(--size, 120px);
    max-width: var(--size, 120px);
    max-height: var(--size, 120px);
    overflow: hidden;
    border-radius: 6px;
    background: var(--bg-tertiary);
    flex-shrink: 0;
    display: grid;
  }

  .placeholder {
    width: 100%;
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-muted);
  }

  .cover {
    object-fit: cover;
    max-width: 100%;
    max-height: 100%;
  }

  /* Single cover - full size */
  .collage.single {
    grid-template: 1fr / 1fr;
  }
  .cover.full {
    width: 100%;
    height: 100%;
  }

  /* 2 covers - side by side */
  .collage.dual {
    grid-template: 1fr / 1fr 1fr;
  }
  .cover.half-left,
  .cover.half-right {
    width: 100%;
    height: 100%;
  }

  /* 3 covers - one big left, two small right */
  .collage.triple {
    grid-template: 1fr 1fr / 1fr 1fr;
  }
  .collage.triple .half-left {
    grid-row: 1 / 3;
    grid-column: 1;
    width: 100%;
    height: 100%;
  }
  .collage.triple .top-right {
    grid-row: 1;
    grid-column: 2;
    width: 100%;
    height: 100%;
  }
  .collage.triple .bottom-right {
    grid-row: 2;
    grid-column: 2;
    width: 100%;
    height: 100%;
  }

  /* 4 covers - 3 small left stacked, 1 large right */
  .collage.quad {
    grid-template-rows: 1fr 1fr 1fr;
    grid-template-columns: 1fr 2fr;
    gap: 2px;
  }
  .small-top {
    grid-row: 1;
    grid-column: 1;
    width: 100%;
    height: 100%;
  }
  .small-mid {
    grid-row: 2;
    grid-column: 1;
    width: 100%;
    height: 100%;
  }
  .small-bot {
    grid-row: 3;
    grid-column: 1;
    width: 100%;
    height: 100%;
  }
  .large-right {
    grid-row: 1 / 4;
    grid-column: 2;
    width: 100%;
    height: 100%;
  }
</style>
