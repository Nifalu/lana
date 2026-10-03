import { isTauri } from '@tauri-apps/api/core';
import type { LngLat } from '../state/app.svelte';

export type Position = { coords: LngLat; accuracyM: number };

export type LocationErrorKind = 'denied' | 'unavailable' | 'timeout';

export class LocationError extends Error {
  readonly kind: LocationErrorKind;
  constructor(kind: LocationErrorKind, message?: string) {
    super(message ?? kind);
    this.name = 'LocationError';
    this.kind = kind;
  }
}

const OPTIONS = { enableHighAccuracy: true, timeout: 10_000, maximumAge: 30_000 };

const isMobileTauri = () =>
  isTauri() && /Android|iPhone|iPad|iPod/.test(navigator.userAgent);

/**
 * One position fix from the best provider available:
 *  - Tauri on a phone: the native geolocation plugin, falling back to the
 *    browser API when the plugin is not registered on the Rust side.
 *  - everything else: `navigator.geolocation`.
 * Rejects with a `LocationError`.
 */
export async function getPosition(): Promise<Position> {
  if (isMobileTauri()) {
    try {
      return await getTauriPosition();
    } catch (err) {
      // A real permission or timeout answer is final. Anything else
      // (plugin missing, service off) gets one more try in the webview.
      if (err instanceof LocationError && err.kind !== 'unavailable') throw err;
      console.warn('native geolocation failed, trying the browser API', err);
    }
  }
  return getBrowserPosition();
}

async function getTauriPosition(): Promise<Position> {
  const { checkPermissions, requestPermissions, getCurrentPosition } = await import(
    '@tauri-apps/plugin-geolocation'
  );
  let status = await checkPermissions();
  if (status.location !== 'granted') {
    status = await requestPermissions(['location']);
    if (status.location !== 'granted') throw new LocationError('denied');
  }
  try {
    const p = await getCurrentPosition(OPTIONS);
    return { coords: [p.coords.longitude, p.coords.latitude], accuracyM: p.coords.accuracy };
  } catch (err) {
    throw new LocationError(classify(err), String(err));
  }
}

function getBrowserPosition(): Promise<Position> {
  return new Promise((resolve, reject) => {
    if (!('geolocation' in navigator)) {
      reject(new LocationError('unavailable'));
      return;
    }
    navigator.geolocation.getCurrentPosition(
      (p) =>
        resolve({ coords: [p.coords.longitude, p.coords.latitude], accuracyM: p.coords.accuracy }),
      (e) => {
        // GeolocationPositionError: 1 denied, 2 unavailable, 3 timeout.
        reject(
          new LocationError(
            e.code === 1 ? 'denied' : e.code === 3 ? 'timeout' : 'unavailable',
            e.message,
          ),
        );
      },
      OPTIONS,
    );
  });
}

/** Native errors are plain strings; guess the kind from the text. */
function classify(err: unknown): LocationErrorKind {
  const text = String(err).toLowerCase();
  if (text.includes('denied') || text.includes('permission')) return 'denied';
  if (text.includes('timeout') || text.includes('timed out')) return 'timeout';
  return 'unavailable';
}
