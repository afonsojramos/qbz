<script lang="ts">
  import type { Snippet } from 'svelte';
  import { t } from '$lib/i18n';
  import { ArrowRight } from 'lucide-svelte';

  interface Props {
    title: string;
    onSeeAll?: () => void;
    children: Snippet;
  }

  let { title, onSeeAll, children }: Props = $props();
</script>

<section class="section">
  <header class="head">
    <h2 class="title">{title}</h2>
    {#if onSeeAll}
      <button class="see-all" type="button" onclick={onSeeAll}>
        {$t('discovery.seeAll')}
        <ArrowRight size={14} />
      </button>
    {/if}
  </header>
  <div class="grid">
    {@render children()}
  </div>
</section>

<style>
  /* Discovery V2 — zero effects.
     Inline CSS grid. No horizontal scroll container, no overflow:auto,
     no scroll handlers. Cards wrap onto rows naturally. */
  .section {
    margin-bottom: 32px;
  }

  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 12px;
  }

  .title {
    font-size: 18px;
    font-weight: 600;
    color: var(--text-primary);
    margin: 0;
  }

  .see-all {
    display: flex;
    align-items: center;
    gap: 4px;
    background: none;
    border: none;
    color: var(--text-muted);
    font-size: 13px;
    cursor: pointer;
    padding: 4px 8px;
    font-family: inherit;
  }

  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
    gap: 16px;
  }
</style>
