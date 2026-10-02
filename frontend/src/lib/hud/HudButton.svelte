<script lang="ts">
  import type { Snippet } from 'svelte';

  let {
    label,
    active = false,
    onclick,
    children,
  }: {
    label: string;
    active?: boolean;
    onclick?: () => void;
    children: Snippet;
  } = $props();
</script>

<button
  class="hud-button"
  class:active
  aria-label={label}
  aria-pressed={active}
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
    backdrop-filter: blur(8px);
    box-shadow: 0 1px 4px rgba(0, 0, 0, 0.2);
    cursor: pointer;
    display: grid;
    place-items: center;
  }

  .hud-button.active {
    background: #1f6feb;
    color: white;
  }

  @media (pointer: coarse) {
    .hud-button {
      width: 48px;
      height: 48px;
    }
  }
</style>
