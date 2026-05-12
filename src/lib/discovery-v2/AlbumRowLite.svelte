<script lang="ts">
  import { cachedSrc } from '$lib/actions/cachedImage';

  interface Props {
    albumId: string;
    title: string;
    artist: string;
    artwork?: string;
    onClick?: () => void;
    onArtistClick?: () => void;
  }

  let {
    albumId,
    title,
    artist,
    artwork,
    onClick,
    onArtistClick,
  }: Props = $props();

  function handleRowClick(e: MouseEvent) {
    if ((e.target as HTMLElement).closest('.artist-link')) return;
    onClick?.();
  }

  function handleArtist(e: MouseEvent) {
    e.stopPropagation();
    onArtistClick?.();
  }
</script>

<div
  class="album-row"
  data-album-id={albumId}
  role="button"
  tabindex="0"
  onclick={handleRowClick}
  onkeydown={(e) => e.key === 'Enter' && onClick?.()}
>
  <div class="thumb">
    {#if artwork}
      <img use:cachedSrc={artwork} alt={title} loading="lazy" decoding="async" />
    {:else}
      <div class="thumb-placeholder"></div>
    {/if}
  </div>
  <div class="text">
    <div class="title">{title}</div>
    {#if onArtistClick}
      <button class="artist-link" type="button" onclick={handleArtist}>{artist}</button>
    {:else}
      <div class="artist">{artist}</div>
    {/if}
  </div>
</div>

<style>
  /* Compact album row — same visual structure as TrackRowLite but click
     navigates to the album instead of playing a track. Used in the
     "Popular albums" section to mirror the dense 4×3 layout from the
     reference UI without dedicating full 220px album cards to it. */
  .album-row {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px 10px;
    border-radius: 6px;
    cursor: pointer;
    background: transparent;
    border: none;
    text-align: left;
    min-width: 0;
  }

  .album-row:hover {
    background: var(--bg-tertiary);
  }

  .thumb {
    flex: 0 0 44px;
    width: 44px;
    height: 44px;
    background: var(--bg-tertiary);
    border-radius: 4px;
    overflow: hidden;
  }

  .thumb img,
  .thumb-placeholder {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }

  .text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .title {
    font-size: 13px;
    font-weight: 500;
    color: var(--text-primary);
    line-height: 1.3;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .artist,
  .artist-link {
    font-size: 12px;
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
</style>
