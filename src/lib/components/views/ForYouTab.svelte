<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { Music, User, Loader2, ArrowRight, Heart, Play, Share2 } from 'lucide-svelte';
  import { cachedSrc } from '$lib/actions/cachedImage';
  import { t } from '$lib/i18n';
  import HorizontalScrollRow from '../HorizontalScrollRow.svelte';
  import AlbumCard from '../AlbumCard.svelte';
  import TrackRow from '../TrackRow.svelte';
  import { formatDuration, formatQuality, getQobuzImage, getQobuzImageForSize } from '$lib/adapters/qobuzAdapters';
  import { resolveArtistImage } from '$lib/stores/customArtistImageStore';
  import { isBlacklisted as isArtistBlacklisted } from '$lib/stores/artistBlacklistStore';
  import { setPlaybackContext } from '$lib/stores/playbackContextStore';
  import type { OfflineCacheStatus } from '$lib/stores/offlineCacheState';
  import type { DisplayTrack, QobuzArtist } from '$lib/types';

  interface AlbumCardData {
    id: string;
    artwork: string;
    title: string;
    artist: string;
    artistId?: number;
    genre: string;
    quality?: string;
    releaseDate?: string;
  }

  interface ArtistCardData {
    id: number;
    name: string;
    image?: string;
    playCount?: number;
  }

  interface Props {
    // Shared data from HomeView (already loaded)
    recentAlbums: AlbumCardData[];
    continueTracks: DisplayTrack[];
    topArtists: ArtistCardData[];
    favoriteAlbums: AlbumCardData[];
    loadingRecentAlbums: boolean;
    loadingContinueTracks: boolean;
    loadingTopArtists: boolean;
    loadingFavoriteAlbums: boolean;
    // Album callbacks
    onAlbumClick?: (albumId: string) => void;
    onAlbumPlay?: (albumId: string) => void;
    onAlbumPlayNext?: (albumId: string) => void;
    onAlbumPlayLater?: (albumId: string) => void;
    onAlbumShareQobuz?: (albumId: string) => void;
    onAlbumShareSonglink?: (albumId: string) => void;
    onAlbumDownload?: (albumId: string) => void;
    onOpenAlbumFolder?: (albumId: string) => void;
    onReDownloadAlbum?: (albumId: string) => void;
    onAddAlbumToPlaylist?: (albumId: string) => void;
    checkAlbumFullyDownloaded?: (albumId: string) => Promise<boolean>;
    downloadStateVersion?: number;
    isAlbumDownloaded: (albumId: string) => boolean;
    loadAlbumDownloadStatus: (albumId: string) => void;
    // Artist callbacks
    onArtistClick?: (artistId: number) => void;
    // Track callbacks
    onTrackPlay?: (track: DisplayTrack) => void;
    onTrackPlayNext?: (track: DisplayTrack) => void;
    onTrackPlayLater?: (track: DisplayTrack) => void;
    onTrackAddToPlaylist?: (trackId: number) => void;
    onTrackShareQobuz?: (trackId: number) => void;
    onTrackShareSonglink?: (track: DisplayTrack) => void;
    onTrackGoToAlbum?: (albumId: string) => void;
    onTrackGoToArtist?: (artistId: number) => void;
    onTrackShowInfo?: (trackId: number) => void;
    onTrackDownload?: (track: DisplayTrack) => void;
    onTrackRemoveDownload?: (trackId: number) => void;
    onTrackReDownload?: (track: DisplayTrack) => void;
    checkTrackDownloaded?: (trackId: number) => boolean;
    getTrackOfflineCacheStatus?: (trackId: number) => { status: OfflineCacheStatus; progress: number };
    activeTrackId?: number | null;
    isPlaybackActive?: boolean;
    // Navigation
    onNavigateDailyQ?: () => void;
    onNavigateWeeklyQ?: () => void;
    onNavigateFavQ?: () => void;
    onNavigateTopQ?: () => void;
  }

  let {
    recentAlbums,
    continueTracks,
    topArtists,
    favoriteAlbums,
    loadingRecentAlbums,
    loadingContinueTracks,
    loadingTopArtists,
    loadingFavoriteAlbums,
    onAlbumClick,
    onAlbumPlay,
    onAlbumPlayNext,
    onAlbumPlayLater,
    onAlbumShareQobuz,
    onAlbumShareSonglink,
    onAlbumDownload,
    onOpenAlbumFolder,
    onReDownloadAlbum,
    onAddAlbumToPlaylist,
    checkAlbumFullyDownloaded,
    downloadStateVersion,
    isAlbumDownloaded,
    loadAlbumDownloadStatus,
    onArtistClick,
    onTrackPlay,
    onTrackPlayNext,
    onTrackPlayLater,
    onTrackAddToPlaylist,
    onTrackShareQobuz,
    onTrackShareSonglink,
    onTrackGoToAlbum,
    onTrackGoToArtist,
    onTrackShowInfo,
    onTrackDownload,
    onTrackRemoveDownload,
    onTrackReDownload,
    checkTrackDownloaded,
    getTrackOfflineCacheStatus,
    activeTrackId = null,
    isPlaybackActive = false,
    onNavigateDailyQ,
    onNavigateWeeklyQ,
    onNavigateFavQ,
    onNavigateTopQ,
  }: Props = $props();

  // For You-specific state
  let failedArtistImages = $state<Set<number>>(new Set());
  let radioLoading = $state<string | null>(null); // album ID currently creating radio

  // Radio Stations: use recent albums as radio seeds
  // Take first 8 recent albums as potential radio stations
  const radioAlbums = $derived(recentAlbums.slice(0, 8));

  async function handleRadioPlay(albumId: string, albumTitle: string) {
    if (radioLoading) return;
    radioLoading = albumId;
    try {
      await invoke('v2_create_album_radio', { albumId, albumName: albumTitle });
    } catch (err) {
      console.error('Failed to create radio:', err);
    } finally {
      radioLoading = null;
    }
  }

  function handleArtistImageError(artistId: number) {
    failedArtistImages = new Set([...failedArtistImages, artistId]);
  }

  function getTrackQuality(track: DisplayTrack): string {
    return formatQuality(track.hires, track.bitDepth, track.samplingRate);
  }

  function buildContinueQueueTracks(tracks: DisplayTrack[]) {
    return tracks.map(track => ({
      id: track.id,
      title: track.title,
      artist: track.artist || 'Unknown Artist',
      album: track.album || '',
      duration_secs: track.durationSeconds,
      artwork_url: track.albumArt || '',
      hires: track.hires ?? false,
      bit_depth: track.bitDepth ?? null,
      sample_rate: track.samplingRate ?? null,
      is_local: track.isLocal ?? false,
      album_id: track.albumId || null,
      artist_id: track.artistId ?? null,
    }));
  }

  async function handleContinueTrackPlay(track: DisplayTrack, trackIndex: number) {
    if (continueTracks.length > 0) {
      const trackIds = continueTracks.map(trk => trk.id);
      await setPlaybackContext(
        'home_list',
        'continue_listening',
        'Continue Listening',
        'qobuz',
        trackIds,
        trackIndex
      );

      try {
        const queueTracks = buildContinueQueueTracks(continueTracks);
        await invoke('v2_set_queue', { tracks: queueTracks, startIndex: trackIndex });
      } catch (err) {
        console.error('Failed to set queue:', err);
      }
    }

    if (onTrackPlay) {
      onTrackPlay(track);
    }
  }

  const hasAnyContent = $derived(
    recentAlbums.length > 0 ||
    continueTracks.length > 0 ||
    topArtists.length > 0 ||
    favoriteAlbums.length > 0
  );

  const anyLoading = $derived(
    loadingRecentAlbums || loadingContinueTracks || loadingTopArtists || loadingFavoriteAlbums
  );
</script>

<!-- Your Mixes -->
<div class="your-mixes-section">
  <h2 class="section-title">{$t('home.yourMixes')}</h2>
  <div class="mix-cards-row">
    <button class="mix-card" onclick={() => onNavigateDailyQ?.()}>
      <div class="mix-card-artwork mix-gradient-daily">
        <span class="mix-card-badge">qobuz</span>
        <span class="mix-card-name">DailyQ</span>
      </div>
      <p class="mix-card-desc">{$t('yourMixes.cardDesc')}</p>
    </button>
    <button class="mix-card" onclick={() => onNavigateWeeklyQ?.()}>
      <div class="mix-card-artwork mix-gradient-weekly">
        <span class="mix-card-badge">qobuz</span>
        <span class="mix-card-name">WeeklyQ</span>
      </div>
      <p class="mix-card-desc">{@html $t('weeklyMixes.cardDesc')}</p>
    </button>
    <button class="mix-card" onclick={() => onNavigateFavQ?.()}>
      <div class="mix-card-artwork mix-gradient-favq">
        <span class="mix-card-badge">qbz</span>
        <span class="mix-card-name">FavQ</span>
      </div>
      <p class="mix-card-desc">{$t('favMixes.cardDesc')}</p>
    </button>
    <button class="mix-card" onclick={() => onNavigateTopQ?.()}>
      <div class="mix-card-artwork mix-gradient-topq">
        <span class="mix-card-badge">qbz</span>
        <span class="mix-card-name">TopQ</span>
      </div>
      <p class="mix-card-desc">{@html $t('topMixes.cardDesc')}</p>
    </button>
  </div>
</div>

<!-- Radio Stations -->
{#if loadingRecentAlbums}
  <div class="skeleton-section">
    <div class="skeleton-title"></div>
    <div class="skeleton-row">
      {#each { length: 4 } as _}<div class="skeleton-card"></div>{/each}
    </div>
  </div>
{:else if radioAlbums.length > 0}
  <div class="section">
    <div class="section-header">
      <h2 class="section-title">{$t('home.radioStations')}</h2>
      <p class="section-subtitle">{$t('home.radioStationsDesc')}</p>
    </div>
    <div class="radio-scroll-row">
      {#each radioAlbums as album (album.id)}
        <button
          class="radio-card"
          class:loading={radioLoading === album.id}
          onclick={() => handleRadioPlay(album.id, album.title)}
          disabled={radioLoading !== null}
        >
          <div class="radio-card-visual">
            <img
              use:cachedSrc={album.artwork}
              alt={album.title}
              class="radio-card-art"
              loading="lazy"
              decoding="async"
            />
            <img
              src="/image_radio_shadows.png"
              alt=""
              class="radio-card-shadow"
            />
            <span class="radio-card-label">{$t('home.radioLabel')}</span>
            {#if radioLoading === album.id}
              <div class="radio-card-loading">
                <Loader2 size={24} class="spinner" />
              </div>
            {/if}
          </div>
          <div class="radio-card-title" title={album.title}>{album.title}</div>
          <div class="radio-card-artist">{album.artist}</div>
        </button>
      {/each}
    </div>
  </div>
{/if}

<!-- Continue Listening -->
{#if loadingContinueTracks}
  <div class="skeleton-section">
    <div class="skeleton-title"></div>
    <div class="skeleton-tracks">
      {#each { length: 5 } as _}<div class="skeleton-track"></div>{/each}
    </div>
  </div>
{:else if continueTracks.length > 0}
  <div class="section">
    <div class="section-header">
      <h2>{$t('home.continueListening')}</h2>
    </div>
    <div class="track-list compact">
      {#each continueTracks as track, index (track.id)}
        {@const isThisActiveTrack = activeTrackId === track.id}
        {@const cacheStatus = getTrackOfflineCacheStatus?.(track.id) ?? { status: 'none' as const, progress: 0 }}
        {@const isTrackDownloaded = cacheStatus.status === 'ready'}
        {@const trackBlacklisted = track.artistId ? isArtistBlacklisted(track.artistId) : false}
        <TrackRow
          trackId={track.id}
          number={index + 1}
          title={track.title}
          artist={track.artist}
          album={track.album}
          duration={track.duration}
          quality={getTrackQuality(track)}
          isPlaying={isPlaybackActive && isThisActiveTrack}
          isActiveTrack={isThisActiveTrack}
          isBlacklisted={trackBlacklisted}
          compact={true}
          hideDownload={trackBlacklisted}
          hideFavorite={trackBlacklisted}
          downloadStatus={cacheStatus.status}
          downloadProgress={cacheStatus.progress}
          onDownload={!trackBlacklisted && onTrackDownload ? () => onTrackDownload(track) : undefined}
          onRemoveDownload={isTrackDownloaded && onTrackRemoveDownload ? () => onTrackRemoveDownload(track.id) : undefined}
          onArtistClick={track.artistId && onArtistClick ? () => onArtistClick(track.artistId!) : undefined}
          onAlbumClick={track.albumId && onAlbumClick ? () => onAlbumClick(track.albumId!) : undefined}
          onPlay={trackBlacklisted ? undefined : () => handleContinueTrackPlay(track, index)}
          menuActions={trackBlacklisted ? {
            onGoToAlbum: track.albumId && onTrackGoToAlbum ? () => onTrackGoToAlbum(track.albumId!) : undefined,
            onGoToArtist: track.artistId && onTrackGoToArtist ? () => onTrackGoToArtist(track.artistId!) : undefined,
            onShowInfo: onTrackShowInfo ? () => onTrackShowInfo(track.id) : undefined
          } : {
            onPlayNow: () => handleContinueTrackPlay(track, index),
            onPlayNext: onTrackPlayNext ? () => onTrackPlayNext(track) : undefined,
            onPlayLater: onTrackPlayLater ? () => onTrackPlayLater(track) : undefined,
            onAddToPlaylist: onTrackAddToPlaylist ? () => onTrackAddToPlaylist(track.id) : undefined,
            onShareQobuz: onTrackShareQobuz ? () => onTrackShareQobuz(track.id) : undefined,
            onShareSonglink: onTrackShareSonglink ? () => onTrackShareSonglink(track) : undefined,
            onGoToAlbum: track.albumId && onTrackGoToAlbum ? () => onTrackGoToAlbum(track.albumId!) : undefined,
            onGoToArtist: track.artistId && onTrackGoToArtist ? () => onTrackGoToArtist(track.artistId!) : undefined,
            onShowInfo: onTrackShowInfo ? () => onTrackShowInfo(track.id) : undefined,
            onDownload: onTrackDownload ? () => onTrackDownload(track) : undefined,
            isTrackDownloaded,
            onReDownload: isTrackDownloaded && onTrackReDownload ? () => onTrackReDownload(track) : undefined,
            onRemoveDownload: isTrackDownloaded && onTrackRemoveDownload ? () => onTrackRemoveDownload(track.id) : undefined
          }}
        />
      {/each}
    </div>
  </div>
{/if}

<!-- Recently Played -->
{#if loadingRecentAlbums}
  <div class="skeleton-section">
    <div class="skeleton-title"></div>
    <div class="skeleton-row">
      {#each { length: 6 } as _}<div class="skeleton-card"></div>{/each}
    </div>
  </div>
{:else if recentAlbums.length > 0}
  <HorizontalScrollRow title={$t('home.recentlyPlayed')}>
    {#snippet children()}
      {#each recentAlbums as album}
        <AlbumCard
          albumId={album.id}
          artwork={album.artwork}
          title={album.title}
          artist={album.artist}
          artistId={album.artistId}
          onArtistClick={onArtistClick}
          genre={album.genre}
          releaseDate={album.releaseDate}
          size="large"
          quality={album.quality}
          onPlay={onAlbumPlay ? () => onAlbumPlay(album.id) : undefined}
          onPlayNext={onAlbumPlayNext ? () => onAlbumPlayNext(album.id) : undefined}
          onPlayLater={onAlbumPlayLater ? () => onAlbumPlayLater(album.id) : undefined}
          onAddAlbumToPlaylist={onAddAlbumToPlaylist ? () => onAddAlbumToPlaylist(album.id) : undefined}
          onShareQobuz={onAlbumShareQobuz ? () => onAlbumShareQobuz(album.id) : undefined}
          onShareSonglink={onAlbumShareSonglink ? () => onAlbumShareSonglink(album.id) : undefined}
          onDownload={onAlbumDownload ? () => onAlbumDownload(album.id) : undefined}
          isAlbumFullyDownloaded={isAlbumDownloaded(album.id)}
          onOpenContainingFolder={onOpenAlbumFolder ? () => onOpenAlbumFolder(album.id) : undefined}
          onReDownloadAlbum={onReDownloadAlbum ? () => onReDownloadAlbum(album.id) : undefined}
          {downloadStateVersion}
          onclick={() => { onAlbumClick?.(album.id); loadAlbumDownloadStatus(album.id); }}
        />
      {/each}
      <div class="spacer"></div>
    {/snippet}
  </HorizontalScrollRow>
{/if}

<!-- Your Top Artists -->
{#if loadingTopArtists}
  <div class="skeleton-section">
    <div class="skeleton-title"></div>
    <div class="skeleton-row">
      {#each { length: 6 } as _}<div class="skeleton-artist"></div>{/each}
    </div>
  </div>
{:else if topArtists.length > 0}
  <HorizontalScrollRow title={$t('home.yourTopArtists')}>
    {#snippet children()}
      {#each topArtists as artist}
        <button class="artist-card" onclick={() => onArtistClick?.(artist.id)}>
          <div class="artist-image-wrapper">
            <div class="artist-image-placeholder">
              <User size={48} />
            </div>
            {#if !failedArtistImages.has(artist.id) && artist.image}
              <img
                use:cachedSrc={artist.image}
                alt={artist.name}
                class="artist-image"
                loading="lazy"
                decoding="async"
                onerror={() => handleArtistImageError(artist.id)}
              />
            {/if}
          </div>
          <div class="artist-name">{artist.name}</div>
          {#if artist.playCount}
            <div class="artist-meta">{$t('home.artistPlays', { values: { count: artist.playCount } })}</div>
          {/if}
        </button>
      {/each}
      <div class="spacer"></div>
    {/snippet}
  </HorizontalScrollRow>
{/if}

<!-- Favorite Albums -->
{#if loadingFavoriteAlbums}
  <div class="skeleton-section">
    <div class="skeleton-title"></div>
    <div class="skeleton-row">
      {#each { length: 6 } as _}<div class="skeleton-card"></div>{/each}
    </div>
  </div>
{:else if favoriteAlbums.length > 0}
  <HorizontalScrollRow title={$t('home.moreFromFavorites')}>
    {#snippet children()}
      {#each favoriteAlbums as album}
        <AlbumCard
          albumId={album.id}
          artwork={album.artwork}
          title={album.title}
          artist={album.artist}
          artistId={album.artistId}
          onArtistClick={onArtistClick}
          genre={album.genre}
          releaseDate={album.releaseDate}
          size="large"
          quality={album.quality}
          onPlay={onAlbumPlay ? () => onAlbumPlay(album.id) : undefined}
          onPlayNext={onAlbumPlayNext ? () => onAlbumPlayNext(album.id) : undefined}
          onPlayLater={onAlbumPlayLater ? () => onAlbumPlayLater(album.id) : undefined}
          onAddAlbumToPlaylist={onAddAlbumToPlaylist ? () => onAddAlbumToPlaylist(album.id) : undefined}
          onShareQobuz={onAlbumShareQobuz ? () => onAlbumShareQobuz(album.id) : undefined}
          onShareSonglink={onAlbumShareSonglink ? () => onAlbumShareSonglink(album.id) : undefined}
          onDownload={onAlbumDownload ? () => onAlbumDownload(album.id) : undefined}
          isAlbumFullyDownloaded={isAlbumDownloaded(album.id)}
          onOpenContainingFolder={onOpenAlbumFolder ? () => onOpenAlbumFolder(album.id) : undefined}
          onReDownloadAlbum={onReDownloadAlbum ? () => onReDownloadAlbum(album.id) : undefined}
          {downloadStateVersion}
          onclick={() => { onAlbumClick?.(album.id); loadAlbumDownloadStatus(album.id); }}
        />
      {/each}
      <div class="spacer"></div>
    {/snippet}
  </HorizontalScrollRow>
{/if}

<!-- Empty state -->
{#if !anyLoading && !hasAnyContent}
  <div class="home-state">
    <div class="state-icon">
      <Music size={48} />
    </div>
    <h1>{$t('home.startListening')}</h1>
    <p>{$t('home.startListeningDescription')}</p>
  </div>
{/if}

<style>
  /* ---- Section layout ---- */
  .section {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .section-header {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .section-header h2 {
    font-size: 20px;
    font-weight: 700;
    color: var(--text-primary);
    margin: 0;
  }

  .section-subtitle {
    font-size: 13px;
    color: var(--text-muted);
    margin: 0;
  }

  .section-title {
    font-size: 20px;
    font-weight: 700;
    color: var(--text-primary);
    margin: 0;
  }

  /* ---- Radio Stations ---- */
  .radio-scroll-row {
    display: flex;
    gap: 16px;
    overflow-x: auto;
    padding-bottom: 4px;
    scrollbar-width: none;
  }

  .radio-scroll-row::-webkit-scrollbar {
    display: none;
  }

  .radio-card {
    flex-shrink: 0;
    width: 180px;
    cursor: pointer;
    background: none;
    border: none;
    padding: 0;
    text-align: left;
    color: inherit;
    transition: opacity 150ms ease;
  }

  .radio-card:disabled {
    cursor: wait;
  }

  .radio-card.loading {
    opacity: 0.7;
  }

  .radio-card-visual {
    position: relative;
    width: 180px;
    height: 180px;
    border-radius: 8px;
    overflow: hidden;
    margin-bottom: 8px;
  }

  .radio-card-art {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .radio-card-shadow {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: cover;
    pointer-events: none;
    mix-blend-mode: multiply;
    opacity: 0.7;
  }

  .radio-card-label {
    position: absolute;
    bottom: 12px;
    left: 0;
    right: 0;
    text-align: center;
    font-size: 22px;
    font-weight: 300;
    letter-spacing: 0.25em;
    color: rgba(255, 255, 255, 0.85);
    text-shadow: 0 2px 8px rgba(0, 0, 0, 0.6);
    pointer-events: none;
  }

  .radio-card-loading {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(0, 0, 0, 0.4);
  }

  .radio-card-title {
    font-size: 13px;
    font-weight: 600;
    color: var(--text-primary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .radio-card-artist {
    font-size: 12px;
    color: var(--text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* ---- Your Mixes (duplicated from HomeView for component isolation) ---- */
  .your-mixes-section {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .mix-cards-row {
    display: flex;
    gap: 16px;
  }

  .mix-card {
    flex-shrink: 0;
    width: 210px;
    cursor: pointer;
    background: none;
    border: none;
    padding: 0;
    text-align: left;
    color: inherit;
  }

  .mix-card-artwork {
    width: 210px;
    height: 210px;
    border-radius: 8px;
    overflow: hidden;
    margin-bottom: 8px;
    position: relative;
    display: flex;
    flex-direction: column;
    justify-content: flex-end;
    padding: 14px;
    box-sizing: border-box;
  }

  .mix-gradient-daily::before,
  .mix-gradient-weekly::before,
  .mix-gradient-favq::before,
  .mix-gradient-topq::before {
    content: '';
    position: absolute;
    inset: -40%;
    will-change: transform;
  }

  .mix-gradient-daily::before {
    background:
      linear-gradient(125deg, transparent 20%, rgba(255, 255, 230, 0.45) 23%, transparent 26%),
      linear-gradient(125deg, transparent 55%, rgba(80, 30, 0, 0.35) 58%, transparent 61%),
      radial-gradient(ellipse at 30% 20%, rgba(255, 255, 255, 0.25) 0%, transparent 50%),
      radial-gradient(ellipse at 70% 60%, rgba(255, 200, 50, 0.4) 0%, transparent 50%),
      radial-gradient(ellipse at 20% 80%, rgba(255, 140, 0, 0.5) 0%, transparent 60%),
      linear-gradient(135deg, #e8a020 0%, #d4781a 30%, #c45e18 60%, #a04010 100%);
    animation: silk-daily 30s ease-in-out infinite alternate;
  }

  .mix-gradient-weekly::before {
    background:
      linear-gradient(125deg, transparent 20%, rgba(255, 220, 255, 0.5) 23%, transparent 26%),
      linear-gradient(125deg, transparent 55%, rgba(30, 0, 50, 0.4) 58%, transparent 61%),
      radial-gradient(ellipse at 40% 20%, rgba(255, 200, 255, 0.35) 0%, transparent 50%),
      radial-gradient(ellipse at 70% 50%, rgba(200, 150, 255, 0.4) 0%, transparent 50%),
      radial-gradient(ellipse at 20% 70%, rgba(130, 80, 200, 0.5) 0%, transparent 60%),
      linear-gradient(135deg, #b060d0 0%, #8040b0 30%, #6030a0 60%, #402080 100%);
    animation: silk-weekly 34s ease-in-out infinite alternate;
  }

  @keyframes silk-daily {
    0%   { transform: translate(5%, 3%) rotate(0deg) scale(1); }
    25%  { transform: translate(-8%, 6%) rotate(6deg) scale(1.03); }
    50%  { transform: translate(3%, -5%) rotate(-4deg) scale(0.98); }
    75%  { transform: translate(-4%, 8%) rotate(8deg) scale(1.02); }
    100% { transform: translate(6%, -3%) rotate(-2deg) scale(1); }
  }

  @keyframes silk-weekly {
    0%   { transform: translate(-3%, 6%) rotate(2deg) scale(1.01); }
    20%  { transform: translate(7%, -4%) rotate(-5deg) scale(0.98); }
    45%  { transform: translate(-6%, -2%) rotate(7deg) scale(1.03); }
    70%  { transform: translate(4%, 7%) rotate(-3deg) scale(1); }
    100% { transform: translate(-5%, 3%) rotate(4deg) scale(0.99); }
  }

  .mix-gradient-favq::before {
    background:
      linear-gradient(125deg, transparent 20%, rgba(255, 200, 200, 0.45) 23%, transparent 26%),
      linear-gradient(125deg, transparent 55%, rgba(80, 0, 0, 0.35) 58%, transparent 61%),
      radial-gradient(ellipse at 30% 20%, rgba(255, 180, 180, 0.25) 0%, transparent 50%),
      radial-gradient(ellipse at 70% 60%, rgba(255, 50, 50, 0.4) 0%, transparent 50%),
      radial-gradient(ellipse at 20% 80%, rgba(200, 0, 0, 0.5) 0%, transparent 60%),
      linear-gradient(135deg, #e82020 0%, #c41818 30%, #a01010 60%, #800808 100%);
    animation: silk-favq 28s ease-in-out infinite alternate;
  }

  .mix-gradient-topq::before {
    background:
      linear-gradient(125deg, transparent 20%, rgba(200, 220, 255, 0.45) 23%, transparent 26%),
      linear-gradient(125deg, transparent 55%, rgba(0, 0, 80, 0.35) 58%, transparent 61%),
      radial-gradient(ellipse at 30% 20%, rgba(180, 200, 255, 0.25) 0%, transparent 50%),
      radial-gradient(ellipse at 70% 60%, rgba(50, 100, 255, 0.4) 0%, transparent 50%),
      radial-gradient(ellipse at 20% 80%, rgba(0, 50, 200, 0.5) 0%, transparent 60%),
      linear-gradient(135deg, #2060e8 0%, #1848c4 30%, #1030a0 60%, #081880 100%);
    animation: silk-topq 32s ease-in-out infinite alternate;
  }

  @keyframes silk-favq {
    0%   { transform: translate(5%, 3%) rotate(0deg) scale(1); }
    25%  { transform: translate(-8%, 6%) rotate(6deg) scale(1.03); }
    50%  { transform: translate(3%, -5%) rotate(-4deg) scale(0.98); }
    75%  { transform: translate(-4%, 8%) rotate(8deg) scale(1.02); }
    100% { transform: translate(6%, -3%) rotate(-2deg) scale(1); }
  }

  @keyframes silk-topq {
    0%   { transform: translate(-3%, 6%) rotate(2deg) scale(1.01); }
    20%  { transform: translate(7%, -4%) rotate(-5deg) scale(0.98); }
    45%  { transform: translate(-6%, -2%) rotate(7deg) scale(1.03); }
    70%  { transform: translate(4%, 7%) rotate(-3deg) scale(1); }
    100% { transform: translate(-5%, 3%) rotate(4deg) scale(0.99); }
  }

  .mix-card-badge {
    position: relative;
    z-index: 1;
    font-size: 11px;
    font-weight: 500;
    color: rgba(255, 255, 255, 0.7);
    letter-spacing: 0.02em;
    margin-bottom: 6px;
  }

  .mix-card-name {
    position: relative;
    z-index: 1;
    font-size: 22px;
    font-weight: 700;
    color: #fff;
    line-height: 1.1;
    text-shadow: 0 1px 4px rgba(0, 0, 0, 0.2);
  }

  .mix-card-desc {
    font-size: 12px;
    font-weight: 400;
    color: var(--text-secondary);
    line-height: 1.4;
    margin: 0;
    min-height: calc(3 * 1.4 * 12px);
  }

  .mix-card-desc :global(strong) {
    font-weight: 600;
    color: var(--text-primary);
  }

  /* ---- Artist cards ---- */
  .artist-card {
    flex-shrink: 0;
    width: 140px;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    background: none;
    border: none;
    cursor: pointer;
    padding: 0;
    color: inherit;
  }

  .artist-image-wrapper {
    position: relative;
    width: 120px;
    height: 120px;
    border-radius: 50%;
    overflow: hidden;
    background: var(--bg-secondary);
  }

  .artist-image-placeholder {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-muted);
  }

  .artist-image {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .artist-name {
    font-size: 13px;
    font-weight: 600;
    color: var(--text-primary);
    text-align: center;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    width: 100%;
  }

  .artist-meta {
    font-size: 12px;
    color: var(--text-muted);
  }

  /* ---- Track list ---- */
  .track-list {
    display: flex;
    flex-direction: column;
  }

  .track-list.compact {
    gap: 0;
  }

  /* ---- Skeleton loading ---- */
  .skeleton-section {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .skeleton-title {
    width: 200px;
    height: 24px;
    border-radius: 4px;
    background: var(--bg-tertiary);
  }

  .skeleton-row {
    display: flex;
    gap: 16px;
  }

  .skeleton-card {
    width: 210px;
    height: 280px;
    border-radius: 8px;
    background: var(--bg-tertiary);
    flex-shrink: 0;
  }

  .skeleton-artist {
    width: 120px;
    height: 160px;
    border-radius: 8px;
    background: var(--bg-tertiary);
    flex-shrink: 0;
  }

  .skeleton-tracks {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .skeleton-track {
    height: 48px;
    border-radius: 4px;
    background: var(--bg-tertiary);
  }

  /* ---- Empty state ---- */
  .home-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    text-align: center;
    padding: 60px 24px;
    color: var(--text-muted);
    gap: 12px;
  }

  .home-state .state-icon {
    opacity: 0.5;
    margin-bottom: 8px;
  }

  .home-state h1 {
    font-size: 20px;
    font-weight: 600;
    color: var(--text-primary);
    margin: 0;
  }

  .home-state p {
    font-size: 14px;
    margin: 0;
    max-width: 320px;
  }

  .spacer {
    width: 16px;
    flex-shrink: 0;
  }

  :global(.spinner) {
    animation: spin 1s linear infinite;
    color: var(--text-primary);
  }

  @keyframes spin {
    from { transform: rotate(0deg); }
    to { transform: rotate(360deg); }
  }
</style>
