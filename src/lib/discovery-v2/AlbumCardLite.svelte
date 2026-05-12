<script lang="ts">
  import { Play } from 'lucide-svelte';
  import { t } from '$lib/i18n';
  import { cachedSrc } from '$lib/actions/cachedImage';
  import type { AlbumRibbon } from './data';

  interface Props {
    albumId: string;
    title: string;
    artist: string;
    artwork?: string;
    quality?: string;
    ribbon?: AlbumRibbon;
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
    quality,
    ribbon,
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
      <img class="cover" use:cachedSrc={artwork} alt={title} loading="lazy" decoding="async" />
    {:else}
      <div class="cover cover-placeholder"></div>
    {/if}
    {#if ribbon}
      <div class="ribbon ribbon-{ribbon.kind}" title={ribbon.label}>{ribbon.label}</div>
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
  {#if quality}
    <div class="quality">{quality}</div>
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
    width: 220px;
    cursor: pointer;
    background: transparent;
    border: none;
    padding: 0;
    text-align: left;
  }

  .cover-wrap {
    position: relative;
    width: 220px;
    height: 220px;
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

  /* Press / award ribbon. Three variants from the original AlbumCard:
     - press: solid gold gradient with dark readable text (most common)
     - qobuzissime: dark scrim with purple accent border
     - albumOfTheWeek: dark scrim with yellow accent border. */
  .ribbon {
    position: absolute;
    top: 8px;
    left: 0;
    padding: 4px 10px;
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: #fff;
    background: rgba(0, 0, 0, 0.88);
    border-top-right-radius: 3px;
    border-bottom-right-radius: 3px;
    border-left: 3px solid var(--accent-primary);
    pointer-events: none;
    max-width: calc(100% - 12px);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .ribbon.ribbon-albumOfTheWeek {
    border-left-color: #eab308;
  }

  .ribbon.ribbon-qobuzissime {
    border-left-color: #8b5cf6;
  }

  .ribbon.ribbon-press {
    background: linear-gradient(135deg, #f5c042 0%, #d49511 100%);
    color: #1f1407;
    border-left: none;
    padding-left: 10px;
    text-shadow: 0 1px 0 rgba(255, 255, 255, 0.15);
  }

  /* Hi-Res / CD Quality badge. Sits at the bottom of the card, below the
     artist line. Small, static, no animation. */
  .quality {
    margin-top: 4px;
    font-family: 'LINE Seed JP', var(--font-sans);
    font-size: 10px;
    font-weight: 400;
    color: var(--alpha-85);
    background: var(--alpha-10);
    border: 1px solid var(--alpha-15);
    border-radius: 4px;
    padding: 3px 6px;
    align-self: flex-start;
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
