import { isTauri } from '@tauri-apps/api/core';
import { FixtureRepository } from './fixture';
import type { Repository } from './repository';
import { TauriRepository } from './tauri';
import type { CacheInfo, Poi, PoiKind, Station, SyncReport } from './types';

/** Where the data currently on screen comes from. */
export type DataSource = 'server' | 'fixture';

/**
 * Plain (non-reactive) indicator, updated by `repository` as it switches
 * between server data and fixtures. The reactive view of it is
 * `connection` in store.svelte.ts.
 */
export const dataSource: { current: DataSource } = { current: 'fixture' };

const fixture = new FixtureRepository();

/**
 * Tauri build: server data through the SQLite cache. While the cache is
 * empty and the server cannot be reached (no server exists yet), fixture
 * data is served instead so the map is never blank. Once one sync has
 * succeeded the app sticks to server data, even when later syncs fail.
 */
class AutoRepository implements Repository {
  private tauri = new TauriRepository();

  async cacheInfo(): Promise<CacheInfo> {
    const info = await this.tauri.cacheInfo();
    if (info.poi_count === 0 && info.station_count === 0 && dataSource.current !== 'server') {
      dataSource.current = 'fixture';
      return fixture.cacheInfo();
    }
    dataSource.current = 'server';
    return info;
  }

  async syncNow(): Promise<SyncReport> {
    try {
      const report = await this.tauri.syncNow();
      dataSource.current = 'server';
      return report;
    } catch (err) {
      const info = await this.tauri.cacheInfo();
      const cacheEmpty = info.poi_count === 0 && info.station_count === 0;
      if (!cacheEmpty) {
        // Offline with cached server data: the caller reports the failure.
        dataSource.current = 'server';
        throw err;
      }
      console.warn('sync failed and the cache is empty, serving fixture data', err);
      dataSource.current = 'fixture';
      return fixture.syncNow();
    }
  }

  listPois(kind?: PoiKind): Promise<Poi[]> {
    return dataSource.current === 'fixture' ? fixture.listPois(kind) : this.tauri.listPois(kind);
  }

  listStations(): Promise<Station[]> {
    return dataSource.current === 'fixture' ? fixture.listStations() : this.tauri.listStations();
  }
}

const forceFixture = import.meta.env.VITE_DATA === 'fixture';

export const repository: Repository = forceFixture || !isTauri() ? fixture : new AutoRepository();
