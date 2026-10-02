import type { Map } from 'maplibre-gl';

/**
 * Ids of the layers this app adds on top of the swisstopo basemap.
 *
 * They are prefixed so they can never collide with layer ids in the
 * basemap style (which has its own `water` layer, for example).
 */
export const LAYER = {
  heat: 'lana-heat',
  water: 'lana-water',
} as const;

export type LayerKey = keyof typeof LAYER;

/**
 * Add every app-owned source and layer to the map.
 *
 * Called on each `style.load`, because switching the basemap with
 * `setStyle` discards all sources and layers that are not part of
 * the new style. Keep everything app-specific in here so a basemap
 * switch restores it completely.
 */
export function addAppLayers(_map: Map): void {
  // TODO: GeoJSON source + LAYER.water (circle/symbol) + LAYER.heat (heatmap)
}

/** Show or hide one app layer. No-op if the layer is not added yet. */
export function setLayerVisible(map: Map, layerId: string, visible: boolean): void {
  if (!map.getLayer(layerId)) {
    console.debug(`filter ${layerId}: ${visible} (layer not added yet)`);
    return;
  }
  map.setLayoutProperty(layerId, 'visibility', visible ? 'visible' : 'none');
}
