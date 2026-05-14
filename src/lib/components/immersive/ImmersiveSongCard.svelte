<script lang="ts">
  import { t } from 'svelte-i18n';
  import QualityBadgeStatic from '$lib/components/QualityBadgeStatic.svelte';

  // Shared bottom-right song card for visualizer panels (LineBed,
  // CometFlow, NeonFlow, EnergyBands, Oscilloscope, Lissajous,
  // SpectralRibbon, TransientPulse, TunnelFlow). Lives here so the
  // structure + styling stays consistent across all 9 panels and any
  // future tweak (badge swap, layout shift) only touches one file.
  //
  // Positioning is owned by the card itself (absolute, bottom-right of
  // the immersive panel that hosts it). Panels just drop the component
  // inside their root and pass the current track props.

  interface Props {
    artwork?: string;
    trackTitle?: string;
    artist?: string;
    album?: string;
    explicit?: boolean;
    quality?: string;
    bitDepth?: number;
    samplingRate?: number;
    format?: string;
  }

  let {
    artwork = '',
    trackTitle = '',
    artist = '',
    album = '',
    explicit = false,
    quality,
    bitDepth,
    samplingRate,
    format,
  }: Props = $props();
</script>

<div class="bottom-info">
  <div class="track-meta">
    <span class="track-title">{trackTitle}</span>
    {#if explicit}
      <span class="explicit-badge" title={$t('library.explicit')}></span>
    {/if}
    <span class="track-subline">
      {#if album}
        <span class="track-album">{album}</span>
        <span class="track-sep" aria-hidden="true">·</span>
      {/if}
      <span class="track-artist">{artist}</span>
    </span>
    <QualityBadgeStatic {quality} {bitDepth} {samplingRate} {format} />
  </div>
  {#if artwork}
    <div class="artwork-thumb">
      <img src={artwork} alt={trackTitle} />
    </div>
  {/if}
</div>

<style>
  .bottom-info {
    position: absolute;
    right: 24px;
    bottom: 24px;
    z-index: 10;
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .track-meta {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 3px;
  }

  .track-title {
    font-size: 15px;
    font-weight: 600;
    color: var(--text-primary, white);
    text-shadow: 0 1px 6px rgba(0, 0, 0, 0.4);
    max-width: 400px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  /* Single subline that joins album + artist with a middle-dot
     separator. Truncation lives on this span (not on each child) so
     the line collapses cleanly when names are long. Children inherit
     the font-size and just contribute color + italic. */
  .track-subline {
    display: inline-block;
    font-size: 12px;
    max-width: 400px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .track-album {
    color: var(--alpha-50, rgba(255, 255, 255, 0.5));
    font-style: italic;
  }

  .track-sep {
    margin: 0 0.4em;
    color: var(--alpha-40, rgba(255, 255, 255, 0.4));
  }

  .track-artist {
    color: var(--alpha-60, rgba(255, 255, 255, 0.6));
  }

  .artwork-thumb {
    width: 72px;
    height: 72px;
    border-radius: 6px;
    overflow: hidden;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.5);
    flex-shrink: 0;
  }

  .artwork-thumb img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .explicit-badge {
    display: inline-block;
    width: 14px;
    height: 14px;
    flex-shrink: 0;
    opacity: 0.45;
    background-color: var(--text-primary, white);
    -webkit-mask: url('/explicit.svg') center / contain no-repeat;
    mask: url('/explicit.svg') center / contain no-repeat;
  }

  @media (max-width: 768px) {
    .bottom-info {
      right: 16px;
      bottom: 16px;
    }

    .artwork-thumb {
      width: 56px;
      height: 56px;
    }

    .track-title {
      font-size: 13px;
      max-width: 220px;
    }
  }
</style>
