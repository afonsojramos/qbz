<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { CircleCheckBig, CircleAlert, TriangleAlert, Info, LoaderCircle, Lock, X } from 'lucide-svelte';
  import {
    isAnyUnlocking,
    subscribe as subscribeUnlocking
  } from '$lib/stores/unlockingStore';

  interface Props {
    message: string;
    type?: 'success' | 'error' | 'info' | 'warning' | 'buffering';
    persistent?: boolean;
    onClose: () => void;
  }

  let { message, type = 'success', persistent = false, onClose }: Props = $props();

  // When the buffering toast is up AND an offline CMAF decrypt is in
  // progress, swap the shimmer label from "Buffering" to a short cycle
  // of unlock-flavored words so the user understands we're unwrapping
  // local encrypted content, not stalling on the network.
  let unlockActive = $state(false);
  const UNLOCK_WORDS = ['Unlocking', 'Decrypting', 'Loading', 'Validating'];
  let unlockWordIdx = $state(0);
  let unlockTimer: ReturnType<typeof setInterval> | null = null;

  function startUnlockCycle() {
    if (unlockTimer) return;
    unlockWordIdx = 0;
    unlockTimer = setInterval(() => {
      unlockWordIdx = (unlockWordIdx + 1) % UNLOCK_WORDS.length;
    }, 900);
  }

  function stopUnlockCycle() {
    if (unlockTimer) {
      clearInterval(unlockTimer);
      unlockTimer = null;
    }
  }

  let unsubscribeUnlock: (() => void) | null = null;
  onMount(() => {
    const refresh = () => {
      const next = isAnyUnlocking();
      if (next !== unlockActive) {
        unlockActive = next;
        if (next) startUnlockCycle();
        else stopUnlockCycle();
      }
    };
    refresh();
    unsubscribeUnlock = subscribeUnlocking(refresh);

    // Don't auto-close persistent toasts (buffering)
    if (persistent) return;

    const timer = setTimeout(onClose, 4000);
    return () => clearTimeout(timer);
  });

  onDestroy(() => {
    unsubscribeUnlock?.();
    stopUnlockCycle();
  });
</script>

<div class="toast" class:success={type === 'success'} class:error={type === 'error'} class:info={type === 'info'} class:warning={type === 'warning'} class:buffering={type === 'buffering'} class:unlocking={type === 'buffering' && unlockActive}>
  <div class="icon">
    {#if type === 'success'}
      <CircleCheckBig size={20} />
    {:else if type === 'error'}
      <CircleAlert size={20} />
    {:else if type === 'warning'}
      <TriangleAlert size={20} />
    {:else if type === 'buffering'}
      {#if unlockActive}
        <Lock size={20} class="lock-jiggle" />
      {:else}
        <LoaderCircle size={20} class="spinning" />
      {/if}
    {:else}
      <Info size={20} />
    {/if}
  </div>
  {#if type === 'buffering'}
    <span class="message">
      <span class="shimmer-text">{unlockActive ? UNLOCK_WORDS[unlockWordIdx] : 'Buffering'}</span>
      <span class="dots">...</span>
      <span class="track-name">{message}</span>
    </span>
  {:else}
    <span class="message">{message}</span>
  {/if}
  <button class="close-btn" onclick={onClose}>
    <X size={16} />
  </button>
</div>

<style>
  .toast {
    position: fixed;
    bottom: 100px;
    left: 50%;
    transform: translateX(-50%);
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px 16px;
    background-color: var(--bg-tertiary);
    border-radius: 8px;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.4);
    z-index: 100;
    animation: slideUp 200ms ease-out;
  }

  @keyframes slideUp {
    from {
      opacity: 0;
      transform: translateX(-50%) translateY(20px);
    }
    to {
      opacity: 1;
      transform: translateX(-50%) translateY(0);
    }
  }

  .icon {
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .toast.success .icon {
    color: #4CAF50;
  }

  .toast.error .icon {
    color: #ff6b6b;
  }

  .toast.info .icon {
    color: var(--accent-primary);
  }

  .toast.warning .icon {
    color: #FFA726;
  }

  .toast.buffering .icon {
    color: var(--accent-primary);
  }

  .toast.buffering .icon :global(.spinning) {
    animation: spin 1s linear infinite;
  }

  @keyframes spin {
    from { transform: rotate(0deg); }
    to { transform: rotate(360deg); }
  }

  .toast.unlocking .icon :global(.lock-jiggle) {
    animation: lock-jiggle 1.2s ease-in-out infinite;
    transform-origin: 50% 70%;
  }

  @keyframes lock-jiggle {
    0%, 100% { transform: rotate(0deg) scale(1); }
    15% { transform: rotate(-10deg) scale(1.05); }
    30% { transform: rotate(10deg) scale(1.05); }
    45% { transform: rotate(-6deg) scale(1.08); }
    60% { transform: rotate(6deg) scale(1.08); }
    75% { transform: rotate(-3deg) scale(1.04); }
  }

  .message {
    font-size: 14px;
    color: var(--text-primary);
  }

  /* Shimmer effect for buffering text */
  .shimmer-text {
    background: linear-gradient(
      90deg,
      var(--text-primary) 0%,
      var(--accent-primary) 25%,
      var(--text-primary) 50%,
      var(--accent-primary) 75%,
      var(--text-primary) 100%
    );
    background-size: 200% 100%;
    -webkit-background-clip: text;
    background-clip: text;
    -webkit-text-fill-color: transparent;
    animation: shimmer 2s ease-in-out infinite;
    font-weight: 500;
  }

  @keyframes shimmer {
    0% { background-position: 200% 0; }
    100% { background-position: -200% 0; }
  }

  .dots {
    color: var(--text-muted);
    margin-right: 8px;
  }

  .track-name {
    color: var(--text-secondary);
  }

  .close-btn {
    background: none;
    border: none;
    color: var(--text-muted);
    cursor: pointer;
    padding: 4px;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: color 150ms ease;
  }

  .close-btn:hover {
    color: var(--text-primary);
  }
</style>
