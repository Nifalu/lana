import type { Basemap } from '../map/basemap';

/**
 * Shared application state.
 *
 * HUD buttons write here; the map side reads it in effects and applies
 * changes to MapLibre. Nothing outside MapView needs the map instance.
 */

/** [longitude, latitude] in WGS84, the order MapLibre uses. */
export type LngLat = [number, number];

/** Basel, Marktplatz. Initial view. */
export const BASEL: LngLat = [7.5886, 47.5581];
export const INITIAL_ZOOM = 13;

/** Which swisstopo style is shown underneath our data. */
export const view = $state({
  basemap: 'standard' as Basemap,
});

/** Layer filters toggled from the HUD. */
export const filters = $state({
  heat: false,
  water: false,
});

/**
 * The user's position.
 *  - `coords`: last known position, or null before any fix.
 *  - `accuracyM`: radius of the position in metres, null for a manual pick.
 *  - `source`: where `coords` came from.
 *  - `status`: state of the current locate request.
 *  - `picking`: debug mode, the next map tap sets `coords` manually.
 */
export const location = $state({
  coords: null as LngLat | null,
  accuracyM: null as number | null,
  source: null as 'gps' | 'manual' | null,
  status: 'idle' as 'idle' | 'locating' | 'error',
  picking: false,
});

/** Assistance request. Backend wiring comes later. */
export const assistance = $state({
  requested: false,
  requestedAt: null as Date | null,
});

export function requestAssistance() {
  assistance.requested = true;
  assistance.requestedAt = new Date();
  // TODO: send to backend (Tauri command) with location.coords
  console.info('assistance requested', { at: assistance.requestedAt, coords: $state.snapshot(location.coords) });
}

export function cancelAssistance() {
  assistance.requested = false;
  assistance.requestedAt = null;
  console.info('assistance cancelled');
}

/**
 * One-shot actions. MapView replaces these with real implementations
 * once the map is ready, so buttons can call them without knowing
 * whether the map exists yet.
 */
export const mapActions = {
  locate: () => {
    console.warn('locate: map not ready yet');
  },
};
