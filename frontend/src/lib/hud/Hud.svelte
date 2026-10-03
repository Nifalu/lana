<script lang="ts">
  import HudButton from './HudButton.svelte';
  import Toast from './Toast.svelte';
  import { filters, location, mapActions, view } from '../state/app.svelte';
  import { sos, triggerSos } from '../sos';

  // Debug tools are on in every build, including the bundle Tauri loads.
  // Release builds turn them off with VITE_RELEASE=true.
  const DEBUG_TOOLS = import.meta.env.VITE_RELEASE !== 'true';

  function toggleBasemap() {
    view.basemap = view.basemap === 'standard' ? 'imagery' : 'standard';
  }
</script>

<div class="hud">
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
  </div>

  {#if location.picking}
    <p class="hint" role="status">Tap the map to set your location</p>
  {/if}

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
  }

  /* Map-level tools (basemap switch, debug) top-right,
     out of the thumb zone because they are used rarely. */
  .tools {
    position: absolute;
    top: calc(env(safe-area-inset-top) + 12px);
    right: calc(env(safe-area-inset-right) + 12px);
    display: flex;
    flex-direction: column;
    gap: 8px;
    font-size: 11px;
    font-weight: 600;
  }

  .hint {
    position: absolute;
    top: calc(env(safe-area-inset-top) + 16px);
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
</style>
