<script lang="ts">
  import HudButton from './HudButton.svelte';
  import Toast from './Toast.svelte';
  import ConnectionIndicator from './ConnectionIndicator.svelte';
  import CoolButton from './CoolButton.svelte';
  import SelectionSheet from './SelectionSheet.svelte';
  import SosSheet from './SosSheet.svelte';
  import TemperatureLegend from './TemperatureLegend.svelte';
  import { filters, location, mapActions, view } from '../state/app.svelte';
  import { helper, sos, sosSheetKind, toggleHelper, triggerSos } from '../sos';
  import { selection } from '../state/selection.svelte';
  import { routing } from '../routing';

  // Debug tools are on in every build, including the bundle Tauri loads.
  // Release builds turn them off with VITE_RELEASE=true.
  const DEBUG_TOOLS = import.meta.env.VITE_RELEASE !== 'true';

  // Height of the selection sheet. On phones the toast is lifted above it.
  let sheetHeight = $state(0);
  let sosHeight = $state(0);
  // An SOS sheet takes the place of the selection sheet.
  const sosOpen = $derived(sosSheetKind() !== null);
  const sheetOpen = $derived(sosOpen || selection.current !== null || routing.current !== null);

  function toggleBasemap() {
    view.basemap = view.basemap === 'standard' ? 'imagery' : 'standard';
  }
</script>

<div class="hud" class:sheet-open={sheetOpen} style:--sheet-h="{sosOpen ? sosHeight : sheetHeight}px">
  <ConnectionIndicator />

  <div class="tools">
    <HudButton
      label={view.basemap === 'imagery' ? 'Show map' : 'Show satellite imagery'}
      active={view.basemap === 'imagery'}
      onclick={toggleBasemap}
    >
      <!-- TODO: replace with the imagery / layers icon SVG -->
      SAT
    </HudButton>

    {#if DEBUG_TOOLS}
      <HudButton
        label={location.picking ? 'Cancel setting location' : 'Debug: set my location by tapping the map'}
        active={location.picking}
        onclick={() => (location.picking = !location.picking)}
      >
        DBG
      </HudButton>
    {/if}

    <HudButton label="Ich kann helfen" active={helper.on} onclick={toggleHelper}>
      <!-- TODO: replace with the helper icon SVG -->
      HILFE
    </HudButton>

    {#if filters.heat}
      <TemperatureLegend />
    {/if}
  </div>

  {#if location.picking}
    <p class="hint" role="status">Tap the map to set your location</p>
  {/if}

  {#if !sosOpen}
    <SelectionSheet bind:height={sheetHeight} />
  {/if}
  <SosSheet bind:height={sosHeight} />

  <Toast />

  <nav class="bar" aria-label="Map tools">
    <HudButton
      label="My location"
      busy={location.status === 'locating'}
      onclick={() => mapActions.locate()}
    >
      <!-- TODO: replace with the location icon SVG -->
      ⌖
    </HudButton>
    <HudButton
      label="Heatmap"
      active={filters.heat}
      onclick={() => (filters.heat = !filters.heat)}
    >
      <!-- TODO: replace with the heat icon SVG -->
      H
    </HudButton>
    <HudButton
      label="Water sources"
      active={filters.water}
      onclick={() => (filters.water = !filters.water)}
    >
      <!-- TODO: replace with the water icon SVG -->
      W
    </HudButton>
    <CoolButton />
    <HudButton
      label={sos.active ? 'Hilfe angefordert' : 'Hilfe anfordern'}
      variant="alert"
      active={sos.active}
      onclick={triggerSos}
    >
      <!-- TODO: replace with the assistance icon SVG -->
      SOS
    </HudButton>
  </nav>
</div>

<style>
  .hud {
    position: absolute;
    inset: 0;
    pointer-events: none;
    /* Extra bottom offset for the toast, read in Toast.svelte. */
    --toast-lift: 0px;
  }

  /* On phones the sheet sits above the bar; the toast goes above the sheet. */
  @media (max-width: 767px) {
    .hud.sheet-open {
      --toast-lift: calc(var(--sheet-h) + 8px);
    }
  }

  /* Map-level tools (basemap switch, debug) top-right,
     out of the thumb zone because they are used rarely. */
  .tools {
    position: absolute;
    top: calc(env(safe-area-inset-top) + 12px);
    right: calc(env(safe-area-inset-right) + 12px);
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 8px;
    font-size: 11px;
    font-weight: 600;
  }

  .hint {
    position: absolute;
    /* Below the connection pill, which owns the top-left corner. */
    top: calc(env(safe-area-inset-top) + 56px);
    left: 50%;
    transform: translateX(-50%);
    margin: 0;
    padding: 8px 14px;
    border-radius: 999px;
    background: rgba(17, 18, 20, 0.75);
    color: #f1f1ef;
    font-size: 13px;
    white-space: nowrap;
  }

  .bar {
    position: absolute;
    left: 50%;
    bottom: calc(env(safe-area-inset-bottom) + 16px);
    transform: translateX(-50%);
    display: flex;
    gap: 12px;
    padding: 8px;
    border-radius: 16px;
    background: rgba(17, 18, 20, 0.6);
    backdrop-filter: blur(12px);
    -webkit-backdrop-filter: blur(12px);
    /* Re-enable clicks on the whole bar so taps between buttons
       don't fall through and start dragging the map. */
    pointer-events: auto;
    font-size: 11px;
    font-weight: 600;
  }

  /* Five buttons plus the cool-spot filter tab must fit a 320 px screen. */
  @media (max-width: 400px) {
    .bar {
      gap: 8px;
    }
  }
</style>
