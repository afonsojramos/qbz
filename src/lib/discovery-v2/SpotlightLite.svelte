<script lang="ts">
  import { Play, User, Radio } from 'lucide-svelte';
  import { t } from '$lib/i18n';
  import { cachedSrc } from '$lib/actions/cachedImage';
  import RadioCardLite from './RadioCardLite.svelte';
  import type { SpotlightSection, SpotlightTopTrack } from './data';

  interface Props {
    spotlight: SpotlightSection;
    onArtistClick?: (artistId: number) => void;
    onPlayTrack?: (track: SpotlightTopTrack) => void;
    onAlbumClick?: (albumId: string) => void;
    onPlaylistClick?: (playlistId: number) => void;
    onStartRadio?: (artistId: number, artistName: string) => void;
  }

  let {
    spotlight,
    onArtistClick,
    onPlayTrack,
    onAlbumClick,
    onPlaylistClick,
    onStartRadio,
  }: Props = $props();

  /**
   * Artist Spotlight hero — a featured-artist deep-dive section. Visual
   * structure (matching the legacy ForYouTab spotlight, simplified):
   *
   *   ┌──────────────────────────────────────────────────────┐
   *   │ [PORTRAIT]   Artist Name                             │
   *   │              category                                │
   *   │              [Top tracks] [Start radio]              │
   *   ├──────────────────────────────────────────────────────┤
   *   │ Top tracks (compact list, up to 5)                   │
   *   │ Albums row (up to 6, AlbumCardLite-style minis)      │
   *   │ Playlists row (if any)                               │
   *   └──────────────────────────────────────────────────────┘
   *
   * Cero efectos beyond a static portrait and clickable rows. No
   * background-color extraction (the legacy did it for the radio card —
   * deferred for V1; the radio button uses theme accent).
   */

  function formatDuration(seconds?: number): string {
    if (!seconds) return '';
    const m = Math.floor(seconds / 60);
    const s = Math.floor(seconds % 60);
    return `${m}:${String(s).padStart(2, '0')}`;
  }
</script>

<section class="spotlight">
  <header class="hero">
    <div class="portrait">
      {#if spotlight.artistImage}
        <img
          use:cachedSrc={spotlight.artistImage}
          alt={spotlight.artistName}
          loading="lazy"
          decoding="async"
        />
      {:else}
        <div class="portrait-placeholder"><User size={48} /></div>
      {/if}
    </div>
    <div class="hero-text">
      <h2 class="artist-name">
        <button
          type="button"
          class="artist-link"
          onclick={() => onArtistClick?.(spotlight.artistId)}
        >
          {spotlight.artistName}
        </button>
      </h2>
      {#if spotlight.category}
        <p class="category">{spotlight.category}</p>
      {/if}
      {#if onStartRadio}
        <button
          class="radio-btn"
          type="button"
          onclick={() => onStartRadio?.(spotlight.artistId, spotlight.artistName)}
        >
          <Radio size={14} />
          {$t('actions.radio.startArtistRadio')}
        </button>
      {/if}
    </div>
  </header>

  {#if spotlight.topTracks.length > 0}
    <div class="block">
      <h3 class="block-title">{$t('artist.popularTracks')}</h3>
      <ul class="tracks">
        {#each spotlight.topTracks as track, idx (track.trackId)}
          <li>
            <button class="track-row" type="button" onclick={() => onPlayTrack?.(track)}>
              <span class="track-rank">{idx + 1}</span>
              <span class="track-play"><Play size={14} fill="currentColor" /></span>
              <span class="track-title">{track.title}</span>
              {#if track.durationSec}
                <span class="track-duration">{formatDuration(track.durationSec)}</span>
              {/if}
            </button>
          </li>
        {/each}
      </ul>
    </div>
  {/if}

  {#if spotlight.albums.length > 0 || onStartRadio}
    <div class="block">
      <h3 class="block-title">{$t('artist.albums')}</h3>
      <div class="albums-row">
        {#if onStartRadio}
          <RadioCardLite
            seedTitle={spotlight.artistName}
            seedSubtitle={$t('discovery.qobuzRadioStation')}
            artwork={spotlight.albums[0]?.artwork ?? spotlight.artistImage}
            onPlay={() => onStartRadio?.(spotlight.artistId, spotlight.artistName)}
            onClick={() => onStartRadio?.(spotlight.artistId, spotlight.artistName)}
          />
        {/if}
        {#each spotlight.albums as album (album.albumId)}
          <button
            class="album-tile"
            type="button"
            onclick={() => onAlbumClick?.(album.albumId)}
          >
            {#if album.artwork}
              <img
                class="album-cover"
                use:cachedSrc={album.artwork}
                alt={album.title}
                loading="lazy"
                decoding="async"
              />
            {:else}
              <div class="album-cover album-cover-placeholder"></div>
            {/if}
            <div class="album-title">{album.title}</div>
            {#if album.releaseYear}<div class="album-year">{album.releaseYear}</div>{/if}
          </button>
        {/each}
      </div>
    </div>
  {/if}

  {#if spotlight.playlists.length > 0}
    <div class="block">
      <h3 class="block-title">{$t('home.qobuzPlaylists')}</h3>
      <div class="playlists-row">
        {#each spotlight.playlists as pl (pl.playlistId)}
          <button
            class="playlist-tile"
            type="button"
            onclick={() => onPlaylistClick?.(pl.playlistId)}
          >
            {#if pl.image}
              <img
                class="playlist-cover"
                use:cachedSrc={pl.image}
                alt={pl.name}
                loading="lazy"
                decoding="async"
              />
            {:else}
              <div class="playlist-cover playlist-cover-placeholder"></div>
            {/if}
            <div class="playlist-name">{pl.name}</div>
          </button>
        {/each}
      </div>
    </div>
  {/if}
</section>

<style>
  .spotlight {
    background: var(--bg-secondary);
    border: 1px solid var(--bg-tertiary);
    border-radius: 12px;
    padding: 24px;
    display: flex;
    flex-direction: column;
    gap: 24px;
  }

  .hero {
    display: flex;
    align-items: center;
    gap: 24px;
  }

  .portrait {
    width: 140px;
    height: 140px;
    flex: 0 0 140px;
    border-radius: 50%;
    overflow: hidden;
    background: var(--bg-tertiary);
  }

  .portrait img,
  .portrait-placeholder {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-muted);
  }

  .hero-text {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
  }

  .artist-name {
    margin: 0;
    font-size: 28px;
    font-weight: 700;
  }

  .artist-link {
    background: none;
    border: none;
    padding: 0;
    color: var(--text-primary);
    font: inherit;
    cursor: pointer;
    text-align: left;
  }

  .category {
    margin: 0;
    font-size: 13px;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .radio-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    background: var(--bg-tertiary);
    border: none;
    color: var(--text-primary);
    padding: 8px 16px;
    border-radius: 20px;
    cursor: pointer;
    font-size: 13px;
    font-weight: 500;
    align-self: flex-start;
    font-family: inherit;
  }

  .radio-btn:hover {
    background: var(--bg-hover);
  }

  .block {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .block-title {
    margin: 0;
    font-size: 16px;
    font-weight: 600;
    color: var(--text-primary);
  }

  .tracks {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .track-row {
    display: grid;
    grid-template-columns: 24px 24px 1fr auto;
    align-items: center;
    gap: 12px;
    padding: 8px 12px;
    background: none;
    border: none;
    border-radius: 6px;
    color: var(--text-primary);
    font-size: 13px;
    cursor: pointer;
    font-family: inherit;
    text-align: left;
    width: 100%;
  }

  .track-row:hover {
    background: var(--bg-tertiary);
  }

  .track-rank {
    color: var(--text-muted);
    font-variant-numeric: tabular-nums;
    transition: opacity 120ms ease;
  }

  .track-row:hover .track-rank {
    opacity: 0;
  }

  .track-play {
    grid-column: 2;
    grid-row: 1;
    color: var(--accent-primary);
    opacity: 0;
    transition: opacity 120ms ease;
    display: flex;
    align-items: center;
  }

  .track-row:hover .track-play {
    opacity: 1;
  }

  /* Stack rank + play in the same cell; only one is visible at a time. */
  .track-row {
    grid-template-columns: 24px 1fr auto;
  }
  .track-rank,
  .track-play {
    grid-column: 1;
    grid-row: 1;
    width: 24px;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .track-title {
    grid-column: 2;
    grid-row: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .track-duration {
    grid-column: 3;
    grid-row: 1;
    color: var(--text-muted);
    font-variant-numeric: tabular-nums;
    font-size: 12px;
  }

  .albums-row,
  .playlists-row {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(140px, 1fr));
    gap: 16px;
  }

  .album-tile,
  .playlist-tile {
    background: none;
    border: none;
    padding: 0;
    cursor: pointer;
    text-align: left;
    font-family: inherit;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .album-cover,
  .playlist-cover {
    width: 100%;
    aspect-ratio: 1;
    object-fit: cover;
    background: var(--bg-tertiary);
    border-radius: 6px;
  }

  .album-cover-placeholder,
  .playlist-cover-placeholder {
    display: block;
  }

  .album-title,
  .playlist-name {
    font-size: 13px;
    color: var(--text-primary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .album-year {
    font-size: 11px;
    color: var(--text-muted);
  }
</style>
