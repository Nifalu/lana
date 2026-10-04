import type { Geometry } from 'geojson';

/**
 * Types mirroring the Rust structs in src-tauri/src/cache.rs, field for
 * field (serde keeps snake_case; timestamps travel as RFC 3339 strings).
 */

export type PoiKind = 'fountain' | 'swim_area' | 'cool_place';

export interface Poi {
  id: number;
  kind: PoiKind;
  name: string;
  /** Which dataset the row came from, e.g. `ods-100270`. */
  source: string;
  source_id: string | null;
  /** Representative point; swim areas also carry a Polygon in `geometry`. */
  lon: number;
  lat: number;
  geometry: Geometry;
  properties: Record<string, unknown>;
}

export interface Station {
  id: string;
  kind: 'air';
  name: string;
  source: string;
  lon: number;
  lat: number;
  geometry: Geometry;
  /** `null` while the station has never reported. */
  temperature_c: number | null;
  measured_at: string | null;
}

export interface CacheInfo {
  last_sync_at: string | null;
  generated_at: string | null;
  poi_count: number;
  station_count: number;
}

export interface SyncReport {
  poi_count: number;
  station_count: number;
  generated_at: string;
}

/** Viewport filter. Field names follow serde (snake_case), not camelCase. */
export interface Bbox {
  min_lon: number;
  min_lat: number;
  max_lon: number;
  max_lat: number;
}
