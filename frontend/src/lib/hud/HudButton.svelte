<script lang="ts">
  import type { Snippet } from 'svelte';

  let {
    label,
    active = false,
    busy = false,
    variant = 'default',
    onclick,
    children,
  }: {
    label: string;
    active?: boolean;
    /** Shows a pulse while an action is in progress. */
    busy?: boolean;
    /** `alert` is for the assistance call: stands out from the tool buttons. */
    variant?: 'default' | 'alert';
    onclick?: () => void;
    children: Snippet;
  } = $props();
</script>

<button
  class="hud-button"
  class:active
  class:alert={variant === 'alert'}
  class:busy
  aria-busy={busy}
  aria-label={label}
  aria-pressed={active}
  type="button"
  {onclick}
>
  {@render children()}
</button>

<style>
  .hud-button {
    pointer-events: auto;
    width: 44px;
    height: 44px;
    border: none;
    border-radius: 10px;
    background: rgba(255, 255, 255, 0.85);
    color: #111214;
    box-shadow: 0 1px 4px rgba(0, 0, 0, 0.2);
    cursor: pointer;
    display: grid;
    place-items: center;
    font: inherit;
    transition: background-color 150ms ease, color 150ms ease;
    -webkit-tap-highlight-color: transparent;
  }

  /* Icon SVGs dropped into the slot inherit the button color
     when they use currentColor for fill or stroke. */
  .hud-button :global(svg) {
    width: 24px;
    height: 24px;
  }

  .hud-button.active {
    background: #1f6feb;
    color: white;
  }

  .hud-button.alert {
    background: #d1342f;
    color: white;
  }

  .hud-button.alert.active {
    background: #7a1d1a;
    animation: alert-pulse 1.2s ease-in-out infinite;
  }

  .hud-button.busy {
    animation: busy-pulse 1s ease-in-out infinite;
  }

  @keyframes busy-pulse {
    0%,
    100% {
      opacity: 1;
    }
    50% {
      opacity: 0.45;
    }
  }

  @keyframes alert-pulse {
    0%,
    100% {
      box-shadow: 0 0 0 0 rgba(209, 52, 47, 0.6);
    }
    50% {
      box-shadow: 0 0 0 8px rgba(209, 52, 47, 0);
    }
  }

  @media (pointer: coarse) {
    .hud-button {
      width: 48px;
      height: 48px;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .hud-button.busy,
    .hud-button.alert.active {
      animation: none;
    }
  }
</style>
