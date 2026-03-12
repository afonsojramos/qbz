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

  // Upscale Qobuz image URLs to larger resolution
  function upscaleImageUrl(url: string): string {
    if (!url) return url;
    return url
      .replace(/_50\.jpg/, '_600.jpg')
      .replace(/_100\.jpg/, '_600.jpg')
      .replace(/_150\.jpg/, '_600.jpg')
      .replace(/_230\.jpg/, '_600.jpg')
      .replace(/_300\.jpg/, '_600.jpg')
      .replace(/\/50x50\//, '/600x600/')
      .replace(/\/100x100\//, '/600x600/')
      .replace(/\/150x150\//, '/600x600/')
      .replace(/\/230x230\//, '/600x600/')
      .replace(/\/300x300\//, '/600x600/');
  }

  // Get unique artworks (dedupe same album covers) and upscale
  const uniqueArtworks = $derived.by(() => {
    const seen = new Set<string>();
    return artworks.filter(art => {
      if (!art || seen.has(art)) return false;
      seen.add(art);
      return true;
    }).slice(0, 4).map(upscaleImageUrl);
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
    <img use:cachedSrc={uniqueArtworks[0]} alt="" class="cover full" />
  {:else if count === 2}
    <img use:cachedSrc={uniqueArtworks[0]} alt="" class="cover half-left" />
    <img use:cachedSrc={uniqueArtworks[1]} alt="" class="cover half-right" />
  {:else if count === 3}
    <img use:cachedSrc={uniqueArtworks[0]} alt="" class="cover half-left" />
    <img use:cachedSrc={uniqueArtworks[1]} alt="" class="cover quarter top-right" />
    <img use:cachedSrc={uniqueArtworks[2]} alt="" class="cover quarter bottom-right" />
  {:else}
    <!-- 4 covers: 3 small stacked left, 1 large right -->
    <img use:cachedSrc={uniqueArtworks[0]} alt="" class="cover small-top" />
    <img use:cachedSrc={uniqueArtworks[1]} alt="" class="cover small-mid" />
    <img use:cachedSrc={uniqueArtworks[2]} alt="" class="cover small-bot" />
    <img use:cachedSrc={uniqueArtworks[3]} alt="" class="cover large-right" />
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
