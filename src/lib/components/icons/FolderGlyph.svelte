<script lang="ts">
  interface Props {
    variant?: 'closed' | 'open' | 'new';
    size?: number;
  }

  let { variant = 'closed', size = 14 }: Props = $props();
</script>

{#if variant === 'open'}
  <svg
    xmlns="http://www.w3.org/2000/svg"
    viewBox="0 0 24 24"
    width={size}
    height={size}
    fill="currentColor"
    aria-hidden="true"
  >
    <path d="M19,8H7A3,3,0,0,0,4.26,9.78L2,14.87V5A2,2,0,0,1,4,3H8a2.05,2.05,0,0,1,1.4.56L11.83,6H17A2,2,0,0,1,19,8Zm2.81,2.42A1,1,0,0,0,21,10H7a1,1,0,0,0-.91.59l-4,9A1,1,0,0,0,3,21H18a1,1,0,0,0,.95-.68l3-9A1,1,0,0,0,21.81,10.42Z" />
  </svg>
{:else if variant === 'new'}
  <svg
    xmlns="http://www.w3.org/2000/svg"
    viewBox="0 0 24 24"
    width={size}
    height={size}
    fill="currentColor"
    aria-hidden="true"
  >
    <path d="M22,7v7.77A3,3,0,0,0,20,14H19V13a3,3,0,0,0-6,0v1H12a3,3,0,0,0-2.23,5H4a2,2,0,0,1-2-2V4A2,2,0,0,1,4,2H9a2,2,0,0,1,1.41.59L12.83,5H20A2,2,0,0,1,22,7Zm-2,9H17V13a1,1,0,0,0-2,0v3H12a1,1,0,0,0,0,2h3v3a1,1,0,0,0,2,0V18h3a1,1,0,0,0,0-2Z" />
  </svg>
{:else}
  <svg
    xmlns="http://www.w3.org/2000/svg"
    viewBox="0 0 24 24"
    width={size}
    height={size}
    fill="currentColor"
    aria-hidden="true"
  >
    <path d="M20,6H13.41L11,3.59A2,2,0,0,0,9.59,3H4A2,2,0,0,0,2,5V19a2,2,0,0,0,2,2H20a2,2,0,0,0,2-2V8A2,2,0,0,0,20,6Z" />
  </svg>
{/if}
