import type { FeatureCollection } from 'geojson';
import type { GeoJSONSource, Map } from 'maplibre-gl';
import type { Route } from '../routing/route';

/**
 * The walking route: a white casing under a blue line. Own source and
 * layers, re-added after every basemap switch (see MapView's `style.load`).
 * Above our overlay layers, below the basemap labels.
 */
const SOURCE = 'lana-route';
const CASING = 'lana-route-casing';
const LINE = 'lana-route-line';
const LINE_STUB = 'lana-route-line-stub';

const EMPTY: FeatureCollection = { type: 'FeatureCollection', features: [] };

// Last value, so a re-added source is filled right away.
let current: FeatureCollection = EMPTY;

export function addRouteLayer(map: Map): void {
  if (map.getSource(SOURCE)) return;
  map.addSource(SOURCE, { type: 'geojson', data: current });

  const beforeId = ['poi_rank2', 'poi_rank1'].find((id) => map.getLayer(id));
  const width = (a: number, b: number): ['interpolate', ['linear'], ['zoom'], 12, number, 18, number] => [
    'interpolate',
    ['linear'],
    ['zoom'],
    12,
    a,
    18,
    b,
  ];
  map.addLayer(
    {
      id: CASING,
      type: 'line',
      source: SOURCE,
      layout: { 'line-cap': 'round', 'line-join': 'round' },
      paint: { 'line-color': '#ffffff', 'line-width': width(6, 12), 'line-opacity': 0.95 },
    },
    beforeId,
  );
  const color = '#1558d6';
  map.addLayer(
    {
      id: LINE,
      type: 'line',
      source: SOURCE,
      filter: ['!=', ['get', 'source'], 'stub'],
      layout: { 'line-cap': 'round', 'line-join': 'round' },
      paint: { 'line-color': color, 'line-width': width(3.5, 7) },
    },
    beforeId,
  );
  // The stub is a straight line, not a path along streets: dash it so it is
  // visibly not a real route. Dashes cannot be data-driven, hence a layer.
  map.addLayer(
    {
      id: LINE_STUB,
      type: 'line',
      source: SOURCE,
      filter: ['==', ['get', 'source'], 'stub'],
      layout: { 'line-join': 'round' },
      paint: { 'line-color': color, 'line-width': width(3.5, 7), 'line-dasharray': [1.5, 1.5] },
    },
    beforeId,
  );
}

/** Pass null to clear the route. */
export function updateRouteLayer(map: Map, route: Route | null): void {
  current = route
    ? {
        type: 'FeatureCollection',
        features: [
          {
            type: 'Feature',
            properties: { source: route.source },
            // Fresh arrays: the route may be a reactive proxy, which cannot
            // be posted to the map's worker.
            geometry: {
              type: 'LineString',
              coordinates: route.line.coordinates.map((c) => [c[0], c[1]]),
            },
          },
        ],
      }
    : EMPTY;
  (map.getSource(SOURCE) as GeoJSONSource | undefined)?.setData(current);
}

/** Frame the route, keeping it clear of the sheet and the top controls. */
export function fitRoute(map: Map, route: Route): void {
  const desktop = window.matchMedia('(min-width: 768px)').matches;
  // The phone sheet grows once the route stats show, so reserve its final
  // height (plus the tool bar below it), but never more than half the map.
  const bottom = Math.min(400, map.getContainer().clientHeight * 0.5);
  // The tool bar is a column on the right edge, or a row along the bottom
  // when a phone is held sideways.
  const sideways = window.matchMedia('(orientation: landscape) and (max-height: 600px)').matches;
  const padding = desktop
    ? { top: 72, bottom: 100, left: 380, right: 80 }
    : { top: 90, bottom, left: 40, right: sideways ? 40 : 90 };
  const coords = route.line.coordinates;
  const [first, ...rest] = coords;
  const bounds = rest.reduce<[number, number, number, number]>(
    (b, [lon, lat]) => [Math.min(b[0], lon), Math.min(b[1], lat), Math.max(b[2], lon), Math.max(b[3], lat)],
    [first[0], first[1], first[0], first[1]],
  );
  map.fitBounds(
    [
      [bounds[0], bounds[1]],
      [bounds[2], bounds[3]],
    ],
    { padding, maxZoom: 17, duration: 600 },
  );
}
