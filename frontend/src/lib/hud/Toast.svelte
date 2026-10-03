<script lang="ts">
  import { fly } from 'svelte/transition';
  import { dismissToast, toast } from './toasts.svelte';
</script>

{#if toast.current}
  {#key toast.current.id}
    <div class="toast" role="status" aria-live="polite" transition:fly={{ y: 12, duration: 180 }}>
      <span>{toast.current.message}</span>
      {#if toast.current.action}
        {@const action = toast.current.action}
        <button
          type="button"
          onclick={() => {
            action.run();
            dismissToast();
          }}
        >
          {action.label}
        </button>
      {/if}
    </div>
  {/key}
{/if}

<style>
  .toast {
    position: absolute;
    left: 50%;
    /* Just above the bottom tool bar, or above the sheet when one is open
       on a phone (the HUD sets --toast-lift). */
    bottom: calc(env(safe-area-inset-bottom) + 92px + var(--toast-lift, 0px));
    transform: translateX(-50%);
    display: flex;
    align-items: center;
    gap: 12px;
    max-width: calc(100vw - 32px);
    padding: 10px 14px;
    border-radius: 12px;
    background: rgba(17, 18, 20, 0.85);
    color: #f1f1ef;
    font-size: 14px;
    box-shadow: 0 2px 10px rgba(0, 0, 0, 0.25);
    pointer-events: auto;
  }

  button {
    border: none;
    background: none;
    color: #7fb2ff;
    font: inherit;
    font-weight: 600;
    padding: 4px 0;
    cursor: pointer;
    white-space: nowrap;
  }
</style>
