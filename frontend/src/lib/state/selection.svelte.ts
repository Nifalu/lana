import type { LngLat } from './app.svelte';

/**
 * Whatever the user tapped on the map: one of our own features
 * (fountain, cool place, swim area, station) or a swisstopo point of interest.
 * `coords` is what gets sent to the routing service.
 */
export type Selection = {
  coords: LngLat;
  /** Display name, null when the feature has none. */
  name: string | null;
  /** Raw kind: our `kind` property, or the swisstopo `subclass`/`class`. */
  kind: string;
  /** German description of the kind, e.g. "Trinkbrunnen". */
  label: string;
  source: 'lana' | 'swisstopo';
  /** Feature properties as the map returned them. */
  properties: Record<string, unknown>;
};

export const selection = $state({ current: null as Selection | null });
