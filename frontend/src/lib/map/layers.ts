import type {
  ExpressionSpecification,
  FilterSpecification,
  GeoJSONSource,
  Map,
} from 'maplibre-gl';
import type { Feature, FeatureCollection } from 'geojson';
import { data } from '../data/store.svelte';
import { dataSource, type DataSource } from '../data';

/**
 * Sources and layers this app adds on top of the swisstopo basemap.
 *
 * Everything is prefixed `lana-` so it can never collide with ids in the
 * basemap style (which has its own `water` layer, for example).
 */
export const SOURCE = {
  pois: 'lana-pois',
  stations: 'lana-stations',
} as const;

export const LAYER = {
  water: 'lana-water',
  swim: 'lana-swim',
  swimOutline: 'lana-swim-outline',
  cool: 'lana-cool',
  heat: 'lana-heat',
  stations: 'lana-stations',
  stationLabels: 'lana-stations-label',
} as const;

/** Which layers each HUD filter shows. */
export const FILTER_LAYERS = {
  water: [LAYER.water, LAYER.swim, LAYER.swimOutline],
  heat: [LAYER.heat, LAYER.stations, LAYER.stationLabels, LAYER.cool],
} as const;

export type FilterKey = keyof typeof FILTER_LAYERS;

/** Basemap symbol layers our layers are inserted below, first one found wins. */
const BELOW_LABELS = ['poi_rank2', 'poi_rank1'];

/** Fonts must exist in the swisstopo glyph set; the POI labels use this one. */
const LABEL_FONT = ['Frutiger Neue Condensed Regular'];

const kindIs = (kind: string): FilterSpecification => ['==', ['get', 'kind'], kind];
const hasTemperature: FilterSpecification = ['==', ['typeof', ['get', 'temperature_c']], 'number'];

const temperature: ExpressionSpecification = ['to-number', ['get', 'temperature_c'], 20];

/** Credit lines shown by MapLibre's attribution control for each data source. */
const BS = 'Daten: Kanton Basel-Stadt';
const OSM = '© OpenStreetMap contributors';

function poisAttribution(source: DataSource): string {
  // The fixture fountains come from OpenStreetMap; the server's from data.bs.ch.
  return source === 'fixture' ? `${OSM} · ${BS}` : BS;
}

export function attributionFor(source: DataSource): { pois: string; stations: string } {
  return { pois: poisAttribution(source), stations: BS };
}

export function poiCollection(): FeatureCollection {
  return {
    type: 'FeatureCollection',
    features: data.pois.map(
      (poi): Feature => ({
        type: 'Feature',
        id: poi.id,
        geometry: poi.geometry,
        properties: {
          ...poi.properties,
          id: poi.id,
          kind: poi.kind,
          name: poi.name,
          source: poi.source,
        },
      }),
    ),
  };
}

export function stationCollection(): FeatureCollection {
  return {
    type: 'FeatureCollection',
    features: data.stations.map(
      (s): Feature => ({
        type: 'Feature',
        id: s.id,
        geometry: s.geometry,
        properties: {
          id: s.id,
          kind: s.kind,
          name: s.name,
          source: s.source,
          temperature_c: s.temperature_c,
          measured_at: s.measured_at,
        },
      }),
    ),
  };
}

/** Push the current store contents into the map's sources (no-op for missing ones). */
export function updateAppData(map: Map): void {
  const attribution = attributionFor(dataSource.current);
  const pois = map.getSource<GeoJSONSource>(SOURCE.pois);
  const stations = map.getSource<GeoJSONSource>(SOURCE.stations);
  if (pois) {
    pois.setData(poiCollection());
    pois.attribution = attribution.pois;
  }
  if (stations) {
    stations.setData(stationCollection());
    stations.attribution = attribution.stations;
  }
}

/**
 * Add every app-owned source and layer to the map.
 *
 * Called on each `style.load`, because switching the basemap with
 * `setStyle` discards all sources and layers that are not part of
 * the new style. Keep everything app-specific in here so a basemap
 * switch restores it completely. Safe to call twice.
 *
 * All layers start hidden; `applyFilters` shows them. The data is
 * already in the sources, so toggling a filter never touches the network.
 */
export function addAppLayers(map: Map): void {
  const attribution = attributionFor(dataSource.current);

  if (!map.getSource(SOURCE.pois)) {
    map.addSource(SOURCE.pois, {
      type: 'geojson',
      data: poiCollection(),
      promoteId: 'id',
      attribution: attribution.pois,
    });
  }
  if (!map.getSource(SOURCE.stations)) {
    map.addSource(SOURCE.stations, {
      type: 'geojson',
      data: stationCollection(),
      promoteId: 'id',
      attribution: attribution.stations,
    });
  }

  const before = BELOW_LABELS.find((id) => map.getLayer(id));
  const add = (layer: Parameters<Map['addLayer']>[0]) => {
    if (!map.getLayer(layer.id)) map.addLayer(layer, before);
  };
  const hidden = { visibility: 'none' } as const;

  // Swim areas first, so their fill sits under every marker.
  add({
    id: LAYER.swim,
    type: 'fill',
    source: SOURCE.pois,
    filter: kindIs('swim_area'),
    layout: { ...hidden },
    paint: { 'fill-color': '#1fc8ff', 'fill-opacity': 0.5 },
  });
  add({
    id: LAYER.swimOutline,
    type: 'line',
    source: SOURCE.pois,
    filter: kindIs('swim_area'),
    layout: { ...hidden, 'line-join': 'round' },
    paint: {
      'line-color': '#0a6fb4',
      'line-width': ['interpolate', ['linear'], ['zoom'], 12, 1.5, 17, 3],
    },
  });

  add({
    id: LAYER.heat,
    type: 'heatmap',
    source: SOURCE.stations,
    filter: hasTemperature,
    layout: { ...hidden },
    paint: {
      // ~20 °C and below barely registers, ~35 °C and above is full weight.
      'heatmap-weight': ['interpolate', ['linear'], temperature, 20, 0, 35, 1],
      'heatmap-intensity': ['interpolate', ['linear'], ['zoom'], 11, 1.3, 15, 2.4],
      'heatmap-radius': ['interpolate', ['exponential', 2], ['zoom'], 10, 28, 13, 80, 15, 160],
      'heatmap-color': [
        'interpolate',
        ['linear'],
        ['heatmap-density'],
        0, 'rgba(255, 235, 59, 0)',
        0.1, 'rgba(255, 235, 59, 0.4)',
        0.3, 'rgba(255, 193, 7, 0.7)',
        0.55, 'rgba(255, 112, 25, 0.85)',
        0.8, 'rgba(230, 60, 40, 0.9)',
        1, 'rgba(180, 20, 30, 0.95)',
      ],
      // Fades out as the stations' own markers take over.
      'heatmap-opacity': ['interpolate', ['linear'], ['zoom'], 12, 0.9, 14, 0.8, 15.5, 0],
    },
  });

  add({
    id: LAYER.cool,
    type: 'circle',
    source: SOURCE.pois,
    filter: kindIs('cool_place'),
    layout: { ...hidden },
    paint: {
      'circle-color': '#12a38a',
      'circle-stroke-color': '#ffffff',
      'circle-stroke-width': 1.5,
      'circle-radius': ['interpolate', ['linear'], ['zoom'], 11, 3.5, 13, 5.5, 15, 8, 18, 11],
    },
  });

  add({
    id: LAYER.stations,
    type: 'circle',
    source: SOURCE.stations,
    filter: hasTemperature,
    layout: { ...hidden },
    paint: {
      'circle-color': [
        'interpolate',
        ['linear'],
        temperature,
        16, '#4a90e2',
        22, '#f6d84a',
        28, '#f08a24',
        34, '#d32f2f',
      ],
      'circle-stroke-color': '#ffffff',
      'circle-stroke-width': 1.5,
      'circle-radius': ['interpolate', ['linear'], ['zoom'], 12, 3, 14, 6, 17, 9],
      'circle-opacity': ['interpolate', ['linear'], ['zoom'], 12, 0, 13.5, 1],
      'circle-stroke-opacity': ['interpolate', ['linear'], ['zoom'], 12, 0, 13.5, 1],
    },
  });

  add({
    id: LAYER.stationLabels,
    type: 'symbol',
    source: SOURCE.stations,
    minzoom: 13.5,
    filter: hasTemperature,
    layout: {
      ...hidden,
      'text-field': [
        'concat',
        ['number-format', temperature, { 'min-fraction-digits': 1, 'max-fraction-digits': 1 }],
        '°',
      ],
      'text-font': LABEL_FONT,
      'text-size': ['interpolate', ['linear'], ['zoom'], 13.5, 11, 17, 14],
      'text-anchor': 'top',
      'text-offset': [0, 0.9],
      'text-optional': true,
    },
    paint: {
      'text-color': '#2b2b2b',
      'text-halo-color': 'rgba(255, 255, 255, 0.9)',
      'text-halo-width': 1.5,
    },
  });

  add({
    id: LAYER.water,
    type: 'circle',
    source: SOURCE.pois,
    filter: kindIs('fountain'),
    layout: { ...hidden },
    paint: {
      'circle-color': '#1b8ae0',
      'circle-stroke-color': '#ffffff',
      'circle-stroke-width': 1.5,
      'circle-radius': ['interpolate', ['linear'], ['zoom'], 11, 2.5, 13, 4, 15, 6.5, 18, 10],
    },
  });
}

/** Show or hide one app layer. No-op if the layer is not added yet. */
export function setLayerVisible(map: Map, layerId: string, visible: boolean): void {
  if (!map.getLayer(layerId)) {
    console.debug(`filter ${layerId}: ${visible} (layer not added yet)`);
    return;
  }
  map.setLayoutProperty(layerId, 'visibility', visible ? 'visible' : 'none');
}

/** Show or hide every layer belonging to one HUD filter. */
export function setFilterVisible(map: Map, filter: FilterKey, visible: boolean): void {
  for (const layerId of FILTER_LAYERS[filter]) setLayerVisible(map, layerId, visible);
}
