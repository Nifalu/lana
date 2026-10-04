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

/**
 * One-shot actions. MapView replaces these with real implementations
 * once the map is ready, so buttons can call them without knowing
 * whether the map exists yet.
 */
export const mapActions = {
  /** Resolves once the attempt is over; `location.coords` is set on success. */
  locate: (): Promise<void> => {
    console.warn('locate: map not ready yet');
    return Promise.resolve();
  },
};
