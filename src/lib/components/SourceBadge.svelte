<script lang="ts">
  import { HardDrive } from 'lucide-svelte';
  import { t } from 'svelte-i18n';

  export type SourceBadgeValue =
    | 'user'
    | 'qobuz_download'
    | 'qobuz_purchase'
    | 'qobuz_streaming'
    | 'plex';

  interface Props {
    value: SourceBadgeValue;
    size?: number;
  }

  let { value, size = 14 }: Props = $props();
</script>

<div
  class="source-badge"
  class:local-badge={value === 'user' || value === 'plex'}
  class:purchase-badge={value === 'qobuz_purchase'}
  title={
    value === 'user'
      ? $t('library.localTrackIndicator')
      : value === 'plex'
        ? $t('library.plexTrackIndicator')
        : value === 'qobuz_purchase'
          ? $t('library.qobuzPurchaseIndicator')
          : $t('library.qobuzTrackIndicator')
  }
>
  {#if value === 'user'}
    <HardDrive {size} />
  {:else if value === 'plex'}
    <img src="/plex-logo.svg" alt="Plex" class="qobuz-badge-icon plex-logo-icon" />
  {:else if value === 'qobuz_purchase'}
    <img src="/qobuz-logo-filled.svg" alt="" class="qobuz-badge-icon" />
  {:else}
    <!-- qobuz_download and qobuz_streaming share the Qobuz visual -->
    <img src="/qobuz-logo-filled.svg" alt="" class="qobuz-badge-icon" />
  {/if}
</div>

<style>
  .source-badge {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    color: var(--text-secondary);
    border-radius: 4px;
  }

  .source-badge.local-badge {
    background: rgba(0, 0, 0, 0.7);
    border-radius: 4px;
    backdrop-filter: blur(4px);
  }

  .source-badge.purchase-badge {
    background: rgba(30, 20, 0, 0.85);
    border-radius: 4px;
    backdrop-filter: blur(4px);
    border: 1px solid rgba(234, 179, 8, 0.5);
  }

  .source-badge.purchase-badge .qobuz-badge-icon {
    filter: brightness(0) saturate(100%) invert(75%) sepia(80%) saturate(500%) hue-rotate(10deg) brightness(105%) contrast(90%);
  }

  .source-badge .qobuz-badge-icon {
    width: 24px;
    height: 24px;
  }

  .source-badge .plex-logo-icon {
    width: 18px;
    height: 18px;
    object-fit: contain;
    filter: drop-shadow(0 1px 1px rgba(0, 0, 0, 0.45));
  }
</style>
