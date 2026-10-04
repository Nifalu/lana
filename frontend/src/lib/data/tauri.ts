import { invoke } from '@tauri-apps/api/core';
import type { Repository } from './repository';
import type { CacheInfo, Poi, PoiKind, Station, SyncReport } from './types';

/** The Rust side (src-tauri/src/lib.rs): one command per method. */
export class TauriRepository implements Repository {
  cacheInfo(): Promise<CacheInfo> {
    return invoke<CacheInfo>('get_cache_info');
  }

  syncNow(): Promise<SyncReport> {
    return invoke<SyncReport>('sync_now');
  }

  listPois(kind?: PoiKind): Promise<Poi[]> {
    return invoke<Poi[]>('list_pois', { kind });
  }

  listStations(): Promise<Station[]> {
    return invoke<Station[]>('list_stations');
  }
}
