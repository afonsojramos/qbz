<script lang="ts">
  import { Play, ListMusic } from 'lucide-svelte';
  import { t } from '$lib/i18n';
  import { cachedSrc } from '$lib/actions/cachedImage';
  import { extractPalette } from '$lib/utils/artworkPalette';

  interface Props {
    playlistId: number;
    name: string;
    image?: string;
    onClick?: () => void;
    onPlay?: () => void;
  }

  let { playlistId, name, image, onClick, onPlay }: Props = $props();

  // Playlist covers are often non-square (rectangle). object-fit:contain
  // keeps the full image visible; we letterbox the gaps with the cover's
  // dominant color so the card doesn't look like an island floating in
  // theme-grey. Palette extraction is cached in artworkPalette.ts so
  // repeated playlists across sections re-use the result.
  let dominantBg = $state<string | undefined>(undefined);

  $effect(() => {
    if (!image) {
      dominantBg = undefined;
      return;
    }
    let cancelled = false;
    void extractPalette(image).then((palette) => {
      if (cancelled) return;
      dominantBg = palette.dominant?.hex ?? undefined;
    });
    return () => {
      cancelled = true;
    };
  });

  function handleCardClick(e: MouseEvent) {
    if ((e.target as HTMLElement).closest('.play-btn')) return;
    onClick?.();
  }

  function handlePlay(e: MouseEvent) {
    e.stopPropagation();
    onPlay?.();
  }
</script>

<div
  class="card"
  data-playlist-id={playlistId}
  role="button"
  tabindex="0"
  onclick={handleCardClick}
  onkeydown={(e) => e.key === 'Enter' && onClick?.()}
>
  <div
    class="cover-wrap"
    style:background-color={dominantBg ?? 'var(--bg-tertiary)'}
  >
    {#if image}
      <img class="cover" use:cachedSrc={image} alt={name} loading="lazy" decoding="async" />
    {:else}
      <div class="cover cover-placeholder">
        <ListMusic size={48} />
      </div>
    {/if}
    <button
      class="play-btn"
      type="button"
      aria-label={$t('actions.play')}
      onclick={handlePlay}
    >
      <Play size={16} fill="currentColor" />
    </button>
  </div>
  <div class="title">{name}</div>
</div>

<style>
  /* Cero efectos. Same dimensions as AlbumCardLite for grid alignment. */
  .card {
    display: flex;
    flex-direction: column;
    gap: 4px;
    width: 220px;
    cursor: pointer;
    background: transparent;
    border: none;
    padding: 0;
    text-align: left;
  }

  /* Background-color is set inline from the cover's dominant color (palette
     extraction in artworkPalette.ts). object-fit: contain preserves aspect
     ratio so non-square playlist art (Apple-Music-style rectangles, Qobuz
     vertical covers, etc.) doesn't get cropped; the dominant-color
     background fills the letterbox. */
  .cover-wrap {
    position: relative;
    width: 220px;
    height: 220px;
    border-radius: 6px;
    overflow: hidden;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .cover {
    max-width: 100%;
    max-height: 100%;
    width: auto;
    height: auto;
    object-fit: contain;
    display: block;
  }

  .cover-placeholder {
    width: 100%;
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-muted);
  }

  .play-btn {
    position: absolute;
    bottom: 8px;
    right: 8px;
    width: 32px;
    height: 32px;
    border-radius: 50%;
    border: none;
    background: var(--accent-primary);
    color: var(--btn-primary-text, #000);
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    padding: 0;
  }

  .title {
    font-size: 14px;
    font-weight: 500;
    color: var(--text-primary);
    line-height: 1.3;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
