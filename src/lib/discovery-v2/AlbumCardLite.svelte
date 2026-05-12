<script lang="ts">
  import { Play } from 'lucide-svelte';
  import { t } from '$lib/i18n';

  interface Props {
    albumId: string;
    title: string;
    artist: string;
    artwork?: string;
    isPlaying?: boolean;
    onClick?: () => void;
    onArtistClick?: () => void;
    onPlay?: () => void;
  }

  let {
    albumId,
    title,
    artist,
    artwork,
    isPlaying = false,
    onClick,
    onArtistClick,
    onPlay,
  }: Props = $props();

  function handleCardClick(e: MouseEvent) {
    if ((e.target as HTMLElement).closest('.play-btn, .artist-link')) return;
    onClick?.();
  }

  function handlePlay(e: MouseEvent) {
    e.stopPropagation();
    onPlay?.();
  }

  function handleArtist(e: MouseEvent) {
    e.stopPropagation();
    onArtistClick?.();
  }
</script>

<div
  class="card"
  class:is-playing={isPlaying}
  data-album-id={albumId}
  role="button"
  tabindex="0"
  onclick={handleCardClick}
  onkeydown={(e) => e.key === 'Enter' && onClick?.()}
>
  <div class="cover-wrap">
    {#if artwork}
      <img class="cover" src={artwork} alt={title} loading="lazy" decoding="async" />
    {:else}
      <div class="cover cover-placeholder"></div>
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
  <div class="title">{title}</div>
  {#if onArtistClick}
    <button class="artist-link" type="button" onclick={handleArtist}>{artist}</button>
  {:else}
    <div class="artist">{artist}</div>
  {/if}
</div>

<style>
  /* Discovery V2 — zero effects.
     No transitions, no hover paint, no will-change, no backdrop-filter,
     no animation, no absolute decoration. Five elements per card. */
  .card {
    display: flex;
    flex-direction: column;
    gap: 4px;
    width: 180px;
    cursor: pointer;
    background: transparent;
    border: none;
    padding: 0;
    text-align: left;
  }

  .cover-wrap {
    position: relative;
    width: 180px;
    height: 180px;
    background: var(--bg-tertiary);
    border-radius: 6px;
    overflow: hidden;
  }

  .cover {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }

  .cover-placeholder {
    width: 100%;
    height: 100%;
  }

  /* Permanent play button. Bottom-right. Static — no hover/transition. */
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

  .artist,
  .artist-link {
    font-size: 13px;
    color: var(--text-muted);
    line-height: 1.3;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    background: none;
    border: none;
    padding: 0;
    text-align: left;
    cursor: pointer;
    font-family: inherit;
  }

  .card.is-playing .title {
    color: var(--accent-primary);
  }
</style>
