import { invoke, isTauri } from '@tauri-apps/api/core';
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

let urlApplied: Promise<void> | null = null;

/** The deployed middleware: map data, SOS and live notifications. */
export const DEFAULT_SERVER_URL = 'https://lana-mw.heitzli.ch';

/** VITE_SERVER_URL when the build sets a non-empty one, else the deployed middleware. */
function configuredServerUrl(): string {
  return import.meta.env.VITE_SERVER_URL?.trim() || DEFAULT_SERVER_URL;
}

/**
 * Apply the server URL to the Tauri shell, which stores it in its cache
 * settings: VITE_SERVER_URL when the build sets one, else the deployed
 * middleware. Applied on every start, so a URL stored by an older build (e.g.
 * a LAN address) cannot linger. Run before the first sync. Safe to call from
 * several places: the work happens once.
 */
export function applyServerUrl(): Promise<void> {
  urlApplied ??= doApplyServerUrl();
  return urlApplied;
}

async function doApplyServerUrl(): Promise<void> {
  const url = configuredServerUrl();
  if (forceFixture || !isTauri()) return;
  try {
    await invoke('set_server_url', { url });
  } catch (err) {
    console.error(`setting the server URL to ${url} failed`, err);
  }
}

/**
 * Base URL of the middleware, without trailing slash. Tauri: the URL stored
 * in the shell (after the configured one was applied). Browser: the
 * configured one directly.
 */
export async function getServerBaseUrl(): Promise<string> {
  let url: string;
  if (isTauri()) {
    await applyServerUrl();
    url = await invoke<string>('get_server_url');
  } else {
    url = configuredServerUrl();
  }
  return url.replace(/\/+$/, '');
}
