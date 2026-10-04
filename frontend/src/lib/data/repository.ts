import type { CacheInfo, Poi, PoiKind, Station, SyncReport } from './types';

/**
 * Where the app gets its map data from. Implementations: the Tauri
 * commands (SQLite cache + server sync) and static JSON fixtures.
 */
export interface Repository {
  cacheInfo(): Promise<CacheInfo>;
  /** Pull fresh data from the server into the cache. Rejects when unreachable. */
  syncNow(): Promise<SyncReport>;
  listPois(kind?: PoiKind): Promise<Poi[]>;
  listStations(): Promise<Station[]>;
}
