<script lang="ts">
  import { tick } from 'svelte';
  import HudButton from './HudButton.svelte';
  import { COOL_FILTERS, cooling, routing, setCoolFilter, startNearestCooling } from '../routing';
  import type { CoolFilter } from '../routing';

  const LONG_PRESS_MS = 500;

  let root = $state<HTMLElement>();
  let more = $state<HTMLButtonElement>();
  let open = $state(false);

  const loading = $derived(routing.status === 'loading');
  const filterLabel = $derived(COOL_FILTERS.find((f) => f.value === cooling.filter)?.label ?? '');

  // A long press opens the chooser; the click that ends it must not also
  // start a route.
  let pressTimer: ReturnType<typeof setTimeout> | undefined;
  let longPressed = false;

  function pressStart(e: PointerEvent) {
    if (e.button !== 0) return;
    longPressed = false;
    clearTimeout(pressTimer);
    pressTimer = setTimeout(() => {
      longPressed = true;
      open = true;
      void focusChoice();
    }, LONG_PRESS_MS);
  }

  function pressEnd() {
    clearTimeout(pressTimer);
  }

  function go() {
    if (longPressed) {
      longPressed = false;
      return;
    }
    open = false;
    if (loading) return;
    void startNearestCooling();
  }

  async function focusChoice() {
    await tick();
    root?.querySelector<HTMLElement>('.chooser [aria-pressed="true"]')?.focus();
  }

  function toggle() {
    open = !open;
    if (open) void focusChoice();
  }

  function choose(filter: CoolFilter) {
    setCoolFilter(filter);
    open = false;
    more?.focus();
  }

  // Esc closes the chooser and keeps the sheet below it open.
  function onkeydown(e: KeyboardEvent) {
    if (e.key !== 'Escape' || !open) return;
    e.preventDefault();
    open = false;
    more?.focus();
  }

  function onpointerdown(e: PointerEvent) {
    if (open && root && !root.contains(e.target as Node)) open = false;
  }
</script>

<svelte:window onkeydowncapture={onkeydown} onpointerdowncapture={onpointerdown} />

<div class="cool" bind:this={root}>
  {#if open}
    <div class="chooser" role="group" aria-label="Zielart für den nächsten kühlen Ort">
      {#each COOL_FILTERS as option (option.value)}
        <button
          type="button"
          aria-pressed={cooling.filter === option.value}
          onclick={() => choose(option.value)}
        >
          {option.label}
        </button>
      {/each}
    </div>
  {/if}

  <!-- The wrapper only watches pointer events bubbling up from the button. -->
  <div
    class="main"
    role="presentation"
    onpointerdown={pressStart}
    onpointerup={pressEnd}
    onpointercancel={pressEnd}
    onpointerleave={pressEnd}
    oncontextmenu={(e) => e.preventDefault()}
  >
    <HudButton label="Zum nächsten kühlen Ort" busy={loading} onclick={go}>KÜHL</HudButton>
  </div>
  <button
    bind:this={more}
    class="more"
    class:filtered={cooling.filter !== 'all'}
    type="button"
    aria-label="Zielart wählen, aktuell: {filterLabel}"
    aria-expanded={open}
    onclick={toggle}
  >
    ▾
  </button>
</div>

<style>
  .cool {
    position: relative;
    display: flex;
    /* Row in a horizontal bar, column in the vertical one (set by the HUD). */
    flex-direction: var(--cool-direction, row);
    gap: 2px;
  }

  .main {
    -webkit-touch-callout: none;
    user-select: none;
    -webkit-user-select: none;
  }

  .more {
    pointer-events: auto;
    width: var(--more-w, 22px);
    height: var(--more-h, 44px);
    padding: 0;
    border: none;
    border-radius: 10px;
    background: rgba(255, 255, 255, 0.85);
    color: #111214;
    box-shadow: 0 1px 4px rgba(0, 0, 0, 0.2);
    font: inherit;
    font-size: 13px;
    cursor: pointer;
    -webkit-tap-highlight-color: transparent;
  }

  /* A filter other than "Alles" is set. */
  .more.filtered {
    background: #1f6feb;
    color: white;
  }

  .chooser {
    position: absolute;
    /* Above the button in a horizontal bar, to its left in the vertical one. */
    right: var(--chooser-right, 0px);
    bottom: var(--chooser-bottom, calc(100% + 16px));
    z-index: 5;
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 168px;
    padding: 6px;
    border-radius: 14px;
    background: rgba(17, 18, 20, 0.85);
    backdrop-filter: blur(12px);
    -webkit-backdrop-filter: blur(12px);
    pointer-events: auto;
  }

  .chooser button {
    min-height: 44px;
    padding: 0 14px;
    border: none;
    border-radius: 10px;
    background: transparent;
    color: #f1f1ef;
    font: inherit;
    font-size: 15px;
    font-weight: 600;
    text-align: left;
    cursor: pointer;
    -webkit-tap-highlight-color: transparent;
  }

  .chooser button[aria-pressed='true'] {
    background: #1f6feb;
    color: white;
  }

  button:focus-visible {
    outline: 2px solid #7fb2ff;
    outline-offset: 2px;
  }

  @media (pointer: coarse) {
    .more {
      height: var(--more-h, 48px);
    }
  }
</style>
