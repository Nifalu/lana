import type { FeatureCollection } from 'geojson';
import type { GeoJSONSource, Map } from 'maplibre-gl';
import type { LngLat } from '../state/app.svelte';

/**
 * Ring around the selected feature. Own source and layer, re-added after
 * every basemap switch (see MapView's `style.load`). Added after our overlay
 * layers so it sits on top of their circles, still below the basemap labels.
 */
const SOURCE = 'lana-selection';
export const SELECTION_LAYER = 'lana-selection-ring';

const EMPTY: FeatureCollection = { type: 'FeatureCollection', features: [] };

// Last value, so a re-added source is filled right away.
let current: FeatureCollection = EMPTY;

export function addSelectionLayer(map: Map): void {
  if (map.getSource(SOURCE)) return;
  map.addSource(SOURCE, { type: 'geojson', data: current });

  const beforeId = ['poi_rank2', 'poi_rank1'].find((id) => map.getLayer(id));
  map.addLayer(
    {
      id: SELECTION_LAYER,
      type: 'circle',
      source: SOURCE,
      paint: {
        'circle-radius': 14,
        'circle-color': '#1f6feb',
        'circle-opacity': 0.12,
        'circle-stroke-color': '#1f6feb',
        'circle-stroke-width': 3,
        'circle-stroke-opacity': 0.95,
        // Keeps the ring a flat circle when the map is tilted or rotated.
        'circle-pitch-alignment': 'viewport',
      },
    },
    beforeId,
  );
}

/** Pass null to remove the highlight. */
export function updateSelectionLayer(map: Map, coords: LngLat | null): void {
  current = coords
    ? {
        type: 'FeatureCollection',
        features: [
          {
            type: 'Feature',
            properties: {},
            geometry: { type: 'Point', coordinates: [coords[0], coords[1]] },
          },
        ],
      }
    : EMPTY;
  (map.getSource(SOURCE) as GeoJSONSource | undefined)?.setData(current);
}
