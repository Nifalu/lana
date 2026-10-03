import type { FeatureCollection } from 'geojson';
import type { GeoJSONSource, Map } from 'maplibre-gl';
import type { LngLat } from '../state/app.svelte';

/**
 * Pulsing red markers at the places where someone asks for help. Own source
 * and layers, re-added after every basemap switch (see MapView's `style.load`).
 * Below the basemap labels, above our overlays.
 */
const SOURCE = 'lana-sos';
const HALO = 'lana-sos-halo';
const CORE = 'lana-sos-core';

const RED = '#d1342f';
const EMPTY: FeatureCollection = { type: 'FeatureCollection', features: [] };
const PULSE_MS = 1600;

// Last value, so a re-added source is filled right away.
let current: FeatureCollection = EMPTY;
let frame = 0;

export function addSosLayer(map: Map): void {
  if (map.getSource(SOURCE)) return;
  map.addSource(SOURCE, { type: 'geojson', data: current });

  const beforeId = ['poi_rank2', 'poi_rank1'].find((id) => map.getLayer(id));
  map.addLayer(
    {
      id: HALO,
      type: 'circle',
      source: SOURCE,
      paint: {
        'circle-radius': 12,
        'circle-color': RED,
        'circle-opacity': 0.35,
        'circle-pitch-alignment': 'viewport',
      },
    },
    beforeId,
  );
  map.addLayer(
    {
      id: CORE,
      type: 'circle',
      source: SOURCE,
      paint: {
        'circle-radius': 8,
        'circle-color': RED,
        'circle-stroke-color': '#ffffff',
        'circle-stroke-width': 3,
        'circle-pitch-alignment': 'viewport',
      },
    },
    beforeId,
  );
  if (current.features.length > 0) pulse(map);
}

/** Show a marker at each point; an empty list clears them. */
export function updateSosLayer(map: Map, points: LngLat[]): void {
  current = {
    type: 'FeatureCollection',
    features: points.map((p) => ({
      type: 'Feature',
      properties: {},
      geometry: { type: 'Point', coordinates: [p[0], p[1]] },
    })),
  };
  (map.getSource(SOURCE) as GeoJSONSource | undefined)?.setData(current);
  if (points.length > 0) pulse(map);
}

/** Animate the halo while there is something to show. */
function pulse(map: Map): void {
  if (frame) return;
  if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) return;
  const tick = (now: number) => {
    frame = 0;
    try {
      if (current.features.length === 0 || !map.getLayer(HALO)) return;
      const t = (now % PULSE_MS) / PULSE_MS;
      map.setPaintProperty(HALO, 'circle-radius', 10 + 16 * t);
      map.setPaintProperty(HALO, 'circle-opacity', 0.45 * (1 - t));
    } catch {
      return; // The map was removed under us.
    }
    frame = requestAnimationFrame(tick);
  };
  frame = requestAnimationFrame(tick);
}
