import { dataSource, repository } from './index';
import type { Poi, Station } from './types';

/**
 * Map data, as the UI sees it. Filled from the repository (SQLite cache or
 * fixtures) without waiting for the network, then refreshed by background
 * syncs. Layers are built from this; toggling a filter never fetches.
 */
export const data = $state({
  pois: [] as Poi[],
  stations: [] as Station[],
  status: 'idle' as 'idle' | 'loading' | 'ready' | 'error',
  lastSyncAt: null as string | null,
});

/**
 * State of the link to the data source, driven by every sync attempt:
 *  - connecting: no sync has finished yet
 *  - live: last sync reached the server
 *  - offline: last sync failed, cached server data is shown
 *  - fixture: bundled test data is shown (browser, or no server yet)
 */
export const connection = $state({
  status: 'connecting' as 'connecting' | 'live' | 'offline' | 'fixture',
  lastSyncAt: null as string | null,
  syncing: false,
});

const REFRESH_MS = 60_000;

/** `generated_at` + source of what `data` currently holds, to skip no-op re-reads. */
let loadedKey: string | null = null;

/** Read pois + stations from the repository into `data`. */
async function load(): Promise<void> {
  const info = await repository.cacheInfo();
  data.lastSyncAt = info.last_sync_at;
  const key = `${dataSource.current}|${info.generated_at}|${info.poi_count}|${info.station_count}`;
  if (key === loadedKey) return;
  const [pois, stations] = await Promise.all([repository.listPois(), repository.listStations()]);
  data.pois = pois;
  data.stations = stations;
  loadedKey = key;
}

let inflight: Promise<void> | null = null;

/** Sync with the server and re-read. Concurrent calls share one run. */
export function syncNow(): Promise<void> {
  inflight ??= runSync().finally(() => {
    inflight = null;
  });
  return inflight;
}

async function runSync(): Promise<void> {
  connection.syncing = true;
  try {
    await repository.syncNow();
    await load();
    data.status = 'ready';
    connection.status = dataSource.current === 'server' ? 'live' : 'fixture';
    connection.lastSyncAt = data.lastSyncAt;
  } catch (err) {
    console.warn('sync failed', err);
    connection.status = 'offline';
    connection.lastSyncAt = data.lastSyncAt;
  } finally {
    connection.syncing = false;
  }
}

let started = false;

/**
 * Load cached data right away, then sync in the background and keep
 * syncing while the app is visible. Call once at startup, in parallel
 * with the map loading.
 */
export async function preload(): Promise<void> {
  if (started) return;
  started = true;

  data.status = 'loading';
  try {
    await load();
    data.status = 'ready';
  } catch (err) {
    console.error('reading cached data failed', err);
    data.status = 'error';
  }
  connection.lastSyncAt = data.lastSyncAt;

  void syncNow();

  setInterval(() => {
    if (document.visibilityState === 'visible') void syncNow();
  }, REFRESH_MS);
  document.addEventListener('visibilitychange', () => {
    if (document.visibilityState === 'visible') void syncNow();
  });
  window.addEventListener('online', () => void syncNow());
}
