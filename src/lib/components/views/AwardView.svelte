<script lang="ts">
  /**
   * AwardView — ports the Qobuz iOS/mobile "Premio" detail screen
   * (laurel hero + award-winning releases). Pulls /award/page for the
   * hero info and uses the embedded releases arrays for the album
   * grids. Matches LabelView's overall structure.
   */
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { t } from '$lib/i18n';
  import { ArrowLeft, Award as AwardIcon, LoaderCircle } from 'lucide-svelte';
  import AlbumCard from '../AlbumCard.svelte';
  import HorizontalScrollRow from '../HorizontalScrollRow.svelte';
  import type { AwardPageData, QobuzAlbum } from '$lib/types';
  import { formatQuality } from '$lib/adapters/qobuzAdapters';

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

  interface ReleasesSection {
    id: string;
    items: AlbumCardData[];
  }

  interface Props {
    awardId: string;
    awardName?: string;
    onBack: () => void;
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
    isAlbumDownloaded?: (albumId: string) => boolean;
    downloadStateVersion?: number;
    onArtistClick?: (artistId: number) => void;
  }

  let {
    awardId,
    awardName,
    onBack,
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
    isAlbumDownloaded,
    downloadStateVersion,
    onArtistClick,
  }: Props = $props();

  let page = $state<AwardPageData | null>(null);
  let sections = $state<ReleasesSection[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let heroImageFailed = $state(false);

  function toAlbumCard(item: Record<string, unknown>): AlbumCardData | null {
    if (!item || typeof item !== 'object') return null;
    const id = (item.id as string) ?? '';
    if (!id) return null;
    const image = item.image as { small?: string; thumbnail?: string; large?: string } | undefined;
    const artists = (item.artists as { id: number; name: string }[]) ?? [];
    const artist = (item.artist as { id?: number; name?: string }) ?? artists[0] ?? {};
    const genre = item.genre as { name?: string } | undefined;
    const audioInfo = item.audio_info as { maximum_bit_depth?: number; maximum_sampling_rate?: number } | undefined;
    const dates = item.dates as { original?: string } | undefined;
    const bitDepth = audioInfo?.maximum_bit_depth ?? 16;
    const quality = formatQuality(
      bitDepth > 16,
      audioInfo?.maximum_bit_depth,
      audioInfo?.maximum_sampling_rate
    );
    return {
      id: String(id),
      artwork: image?.large || image?.small || image?.thumbnail || '',
      title: (item.title as string) ?? '',
      artist: artist?.name ?? 'Unknown Artist',
      artistId: artist?.id,
      genre: genre?.name ?? '',
      quality,
      releaseDate: dates?.original,
    };
  }

  async function loadPage() {
    loading = true;
    error = null;
    try {
      const data = await invoke<AwardPageData>('v2_get_award_page', { awardId });
      page = data;

      const builtSections: ReleasesSection[] = [];
      for (const container of data.releases ?? []) {
        const items = (container.data?.items ?? [])
          .map(toAlbumCard)
          .filter((a): a is AlbumCardData => a !== null);
        if (items.length > 0) {
          builtSections.push({
            id: container.id ?? 'releases',
            items,
          });
        }
      }
      sections = builtSections;
    } catch (err) {
      console.error('[AwardView] failed to load:', err);
      error = String(err);
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    loadPage();
  });

  const displayName = $derived(page?.name ?? awardName ?? '');
  const magazineName = $derived(page?.magazine?.name ?? '');
  const heroImage = $derived(page?.image || page?.magazine?.image || '');

  function getContainerTitle(id: string): string {
    const key = `award.section.${id}`;
    const translated = $t(key);
    if (translated && !translated.startsWith('award.section.')) return translated;
    return $t('award.section.releases');
  }
</script>

<div class="award-detail-view">
  <button class="back-btn" onclick={onBack}>
    <ArrowLeft size={16} />
    <span>{$t('actions.back')}</span>
  </button>

  <header class="award-header">
    <div class="award-image-wrapper">
      {#if heroImage && !heroImageFailed}
        <img
          src={heroImage}
          alt={displayName}
          class="award-image"
          loading="lazy"
          decoding="async"
          onerror={() => (heroImageFailed = true)}
        />
      {:else}
        <div class="award-image-placeholder">
          <AwardIcon size={56} />
        </div>
      {/if}
    </div>
    <div class="award-header-info">
      <div class="award-subtitle">{$t('award.kicker')}</div>
      <h1 class="award-name">{displayName}</h1>
      {#if magazineName}
        <div class="award-magazine">{magazineName}</div>
      {/if}
    </div>
  </header>

  <main class="content">
    {#if loading}
      <div class="loading">
        <LoaderCircle size={28} class="spinner" />
        <p>{$t('album.loading')}</p>
      </div>
    {:else if error}
      <div class="error">
        <p>{$t('favorites.failedLoadFavorites')}</p>
        <p class="error-detail">{error}</p>
        <button class="retry-btn" onclick={loadPage}>{$t('actions.retry')}</button>
      </div>
    {:else if sections.length === 0}
      <div class="empty">
        <p>{$t('award.empty')}</p>
      </div>
    {:else}
      {#each sections as section (section.id)}
        <section class="rail">
          <HorizontalScrollRow title={getContainerTitle(section.id)}>
            {#snippet children()}
              {#each section.items as album (album.id)}
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
                  isAlbumFullyDownloaded={isAlbumDownloaded?.(album.id) ?? false}
                  onOpenContainingFolder={onOpenAlbumFolder ? () => onOpenAlbumFolder(album.id) : undefined}
                  onReDownloadAlbum={onReDownloadAlbum ? () => onReDownloadAlbum(album.id) : undefined}
                  {downloadStateVersion}
                  onclick={() => onAlbumClick?.(album.id)}
                />
              {/each}
              <div class="spacer"></div>
            {/snippet}
          </HorizontalScrollRow>
        </section>
      {/each}
    {/if}
  </main>
</div>

<style>
  /* Mirrors LabelView / ArtistDetailView outer container — same
     paddings, scroll behavior, scrollbar treatment. */
  .award-detail-view {
    padding: 24px;
    padding-top: 0;
    padding-left: 18px;
    padding-right: 8px;
    padding-bottom: 100px;
    overflow-y: auto;
    height: 100%;
  }
  .award-detail-view::-webkit-scrollbar { width: 6px; }
  .award-detail-view::-webkit-scrollbar-track { background: transparent; }
  .award-detail-view::-webkit-scrollbar-thumb { background: var(--bg-tertiary); border-radius: 3px; }
  .award-detail-view::-webkit-scrollbar-thumb:hover { background: var(--text-muted); }

  /* Back button — identical to LabelView */
  .back-btn {
    display: flex; align-items: center; gap: 8px;
    font-size: 14px; color: var(--text-muted);
    background: none; border: none; cursor: pointer;
    margin-top: 8px; margin-bottom: 24px; transition: color 150ms ease;
  }
  .back-btn:hover { color: var(--text-secondary); }

  /* Header — identical layout to LabelView (image + info, gap 24,
     mb 40, 180px circular avatar, same typography scale). */
  .award-header { display: flex; gap: 24px; margin-bottom: 40px; }
  .award-image-wrapper {
    width: 180px; height: 180px; border-radius: 50%;
    overflow: hidden; flex-shrink: 0; background: var(--bg-tertiary);
  }
  .award-image { width: 100%; height: 100%; object-fit: cover; }
  .award-image-placeholder {
    width: 100%; height: 100%;
    display: flex; align-items: center; justify-content: center;
    background: linear-gradient(135deg, #b45309 0%, #eab308 100%); color: white;
  }
  .award-header-info {
    flex: 1; min-width: 0; display: flex; flex-direction: column; justify-content: center;
  }
  .award-subtitle {
    font-size: 12px; font-weight: 600; color: var(--text-muted);
    text-transform: uppercase; letter-spacing: 0.1em; margin-bottom: 4px;
  }
  .award-name {
    font-size: 32px; font-weight: 700; color: var(--text-primary);
    margin: 0 0 8px 0; line-height: 1.2;
  }
  .award-magazine {
    font-size: 14px; color: var(--text-secondary); line-height: 1.4;
  }

  .content {
    display: flex;
    flex-direction: column;
    gap: 48px;
  }
  .rail { display: flex; flex-direction: column; }

  .loading,
  .error,
  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 12px;
    padding: 48px 24px;
    color: var(--text-secondary);
    text-align: center;
  }
  .error-detail { font-size: 12px; color: var(--text-muted); }
  .retry-btn {
    margin-top: 8px;
    padding: 8px 16px;
    background: var(--bg-tertiary);
    border: 1px solid var(--border-primary);
    border-radius: 6px;
    color: var(--text-primary);
    font-size: 13px;
    cursor: pointer;
    transition: background-color 150ms ease;
  }
  .retry-btn:hover { background: var(--bg-secondary); }
  .spacer { width: 8px; flex-shrink: 0; }
</style>
