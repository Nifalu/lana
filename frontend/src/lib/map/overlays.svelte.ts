import { untrack } from 'svelte';
import type { Map } from 'maplibre-gl';
import { data } from '../data/store.svelte';
import { showToast } from '../hud/toasts.svelte';
import { filters } from '../state/app.svelte';
import { updateAppData } from './layers';

/**
 * Reactive glue between the data store, the HUD filters and the map
 * overlays. Call once from a component's script, with a getter for the
 * map (null until the first style has loaded).
 */
export function useOverlays(getMap: () => Map | null): void {
  // New data (cache read, sync) goes straight into the GeoJSON sources.
  $effect(() => {
    const map = getMap();
    if (!map) return;
    updateAppData(map);
  });

  // Heat on, but nothing to show: say so instead of an empty map.
  let heatWasOn = false;
  $effect(() => {
    const heat = filters.heat;
    if (heat && !heatWasOn) {
      untrack(() => {
        const hasReadings = data.stations.some((s) => s.temperature_c != null);
        if (data.status === 'ready' && !hasReadings) showToast('Noch keine Temperaturdaten');
      });
    }
    heatWasOn = heat;
  });
}
