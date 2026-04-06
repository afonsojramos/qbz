<script lang="ts">
  import { t } from '$lib/i18n';
  import { qobuzTosAccepted } from '$lib/stores/qobuzLegalStore';

  interface Props {
    showCheckbox?: boolean;
    showBrandingLine?: boolean;
    showTrademarkNotice?: boolean;
    showTermsLink?: boolean;
    checkboxDisabled?: boolean;
    className?: string;
  }

  let {
    showCheckbox = true,
    showBrandingLine = true,
    showTrademarkNotice = true,
    showTermsLink = false,
    checkboxDisabled = false,
    className = ''
  }: Props = $props();

  const TERMS_URL = 'https://www.qobuz.com/us-en/legal/terms';
</script>

<div class={`qobuz-legal ${className}`}>
  {#if showBrandingLine}
    <p class="branding-line">
      {$t('legal.brandingLine')}
    </p>
  {/if}

  {#if showTrademarkNotice}
    <p class="trademark-line">
      {$t('legal.trademarkNotice')}
    </p>
  {/if}

  {#if showCheckbox}
    <label class="tos-line">
      <input type="checkbox" bind:checked={$qobuzTosAccepted} disabled={checkboxDisabled} />
      <span>
        {$t('legal.tosAgreementPrefix')}
        <a href={TERMS_URL} target="_blank" rel="noopener">
          {$t('legal.tosLinkText')}
        </a>
      </span>
    </label>
  {:else if showTermsLink}
    <p class="tos-link">
      <a href={TERMS_URL} target="_blank" rel="noopener">
        {$t('legal.tosLinkText')}
      </a>
    </p>
  {/if}
</div>

<style>
  .qobuz-legal {
    margin-top: 12px;
  }

  .branding-line {
    font-size: 12px;
    color: var(--text-muted);
    line-height: 1.4;
    margin: 0 0 10px;
  }

  .tos-line {
    display: flex;
    gap: 10px;
    align-items: flex-start;
    font-size: 12px;
    color: var(--text-muted);
    line-height: 1.4;
    user-select: none;
  }

  .tos-line input {
    margin-top: 2px;
  }

  .trademark-line {
    font-size: 12px;
    color: var(--text-muted);
    line-height: 1.4;
    margin: 0 0 10px;
  }

  .tos-line a,
  .tos-link a {
    color: var(--accent-primary);
    text-decoration: none;
  }

  .tos-line a:hover,
  .tos-link a:hover {
    text-decoration: underline;
  }
</style>
