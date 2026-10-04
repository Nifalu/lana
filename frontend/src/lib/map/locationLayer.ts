import type { FeatureCollection, Polygon } from 'geojson';
import type { GeoJSONSource, Map } from 'maplibre-gl';
import type { LngLat } from '../state/app.svelte';

/**
 * Accuracy circle around the user's position. Own source and layers,
 * re-added after every basemap switch (see MapView's `style.load`).
 */
const SOURCE = 'lana-accuracy';
const FILL = 'lana-accuracy-fill';
const LINE = 'lana-accuracy-line';

const EMPTY: FeatureCollection = { type: 'FeatureCollection', features: [] };

// Last value, so a re-added source is filled right away.
let current: FeatureCollection = EMPTY;

export function addLocationLayer(map: Map): void {
  if (map.getSource(SOURCE)) return;
  map.addSource(SOURCE, { type: 'geojson', data: current });

  // Below the labels, so place names stay readable.
  const beforeId = ['poi_rank2', 'poi_rank1'].find((id) => map.getLayer(id));
  map.addLayer(
    {
      id: FILL,
      type: 'fill',
      source: SOURCE,
      paint: { 'fill-color': '#4aa3ff', 'fill-opacity': 0.15 },
    },
    beforeId,
  );
  map.addLayer(
    {
      id: LINE,
      type: 'line',
      source: SOURCE,
      paint: { 'line-color': '#1f6feb', 'line-width': 1, 'line-opacity': 0.5 },
    },
    beforeId,
  );
}

/** Pass null coords or accuracy to hide the circle. */
export function updateLocationLayer(
  map: Map,
  coords: LngLat | null,
  accuracyM: number | null,
): void {
  current =
    coords && accuracyM != null && accuracyM > 0
      ? {
          type: 'FeatureCollection',
          features: [{ type: 'Feature', properties: {}, geometry: circle(coords, accuracyM) }],
        }
      : EMPTY;
  (map.getSource(SOURCE) as GeoJSONSource | undefined)?.setData(current);
}

/** Closed ring of 64 points, `radiusM` metres from `center`. */
function circle([lon, lat]: LngLat, radiusM: number, points = 64): Polygon {
  const dLat = radiusM / 111_320;
  const dLon = dLat / Math.cos((lat * Math.PI) / 180);
  const ring: [number, number][] = [];
  for (let i = 0; i < points; i++) {
    const a = (i / points) * 2 * Math.PI;
    ring.push([lon + dLon * Math.cos(a), lat + dLat * Math.sin(a)]);
  }
  ring.push(ring[0]);
  return { type: 'Polygon', coordinates: [ring] };
}
