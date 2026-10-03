import { showToast } from '../hud/toasts.svelte';
import type { LngLat } from '../state/app.svelte';
import { decodePolyline6 } from './polyline';

export type Route = {
  line: GeoJSON.LineString;
  distanceKm: number;
  durationS: number;
  /** `stub` is a straight line, not a real route. */
  source: 'valhalla' | 'stub';
};

/** Pedestrian speed used for the straight-line stub. */
const WALKING_KMH = 5;
const REQUEST_TIMEOUT_MS = 8000;

/** Routing service base URL, e.g. `http://host:port`. Unset means stub only. */
function baseUrl(): string | null {
  const url = import.meta.env.VITE_ROUTING_URL?.trim();
  return url ? url.replace(/\/+$/, '') : null;
}

/** Great-circle distance in kilometres. */
export function haversineKm([lon1, lat1]: LngLat, [lon2, lat2]: LngLat): number {
  const rad = Math.PI / 180;
  const dLat = (lat2 - lat1) * rad;
  const dLon = (lon2 - lon1) * rad;
  const a =
    Math.sin(dLat / 2) ** 2 + Math.cos(lat1 * rad) * Math.cos(lat2 * rad) * Math.sin(dLon / 2) ** 2;
  return 2 * 6371.0088 * Math.asin(Math.min(1, Math.sqrt(a)));
}

/** Straight line from A to B, walked at 5 km/h. */
export function stubRoute(from: LngLat, to: LngLat): Route {
  const distanceKm = haversineKm(from, to);
  return {
    line: { type: 'LineString', coordinates: [from, to] },
    distanceKm,
    durationS: (distanceKm / WALKING_KMH) * 3600,
    source: 'stub',
  };
}

type ValhallaResponse = {
  trip?: {
    legs?: { shape: string }[];
    summary?: { length: number; time: number };
  };
};

async function fetchValhalla(
  base: string,
  from: LngLat,
  to: LngLat,
  signal?: AbortSignal,
): Promise<Route> {
  const body = {
    locations: [
      { lat: from[1], lon: from[0], type: 'break' },
      { lat: to[1], lon: to[0], type: 'break' },
    ],
    costing: 'pedestrian',
    units: 'kilometers',
    shape_format: 'polyline6',
  };
  const timeout = AbortSignal.timeout(REQUEST_TIMEOUT_MS);
  const response = await fetch(`${base}/route`, {
    method: 'POST',
    // text/plain keeps this a "simple" cross-origin request, so the browser
    // sends no CORS preflight. Valhalla reads the body as JSON regardless.
    headers: { 'Content-Type': 'text/plain' },
    body: JSON.stringify(body),
    signal: signal ? AbortSignal.any([signal, timeout]) : timeout,
  });

  if (!response.ok) {
    // Valhalla errors are JSON: { error_code, error, status_code }.
    const detail = await response
      .json()
      .then((e: { error?: string; error_code?: number }) => `${e.error_code} ${e.error}`)
      .catch(() => response.statusText);
    throw new Error(`routing HTTP ${response.status}: ${detail}`);
  }

  const { trip }: ValhallaResponse = await response.json();
  if (!trip?.legs?.length || !trip.summary) throw new Error('routing: response has no trip');

  // Each leg starts where the previous one ended; drop the duplicate point.
  const coordinates: LngLat[] = [];
  trip.legs.forEach((leg, i) => {
    const points = decodePolyline6(leg.shape);
    coordinates.push(...(i === 0 ? points : points.slice(1)));
  });
  if (coordinates.length < 2) throw new Error('routing: empty shape');

  return {
    line: { type: 'LineString', coordinates },
    distanceKm: trip.summary.length,
    durationS: trip.summary.time,
    source: 'valhalla',
  };
}

/**
 * Walking route from A to B.
 *
 * Without `VITE_ROUTING_URL` this is the straight-line stub. With it, a
 * failed request (network, CORS, 4xx) falls back to the stub too, with a
 * toast, so the UI keeps working. Only an abort by the caller rejects.
 */
export async function route(from: LngLat, to: LngLat, signal?: AbortSignal): Promise<Route> {
  const base = baseUrl();
  if (!base) return stubRoute(from, to);
  try {
    return await fetchValhalla(base, from, to, signal);
  } catch (err) {
    if (signal?.aborted) throw err;
    console.warn('routing failed, using a straight line', err);
    showToast('Routing nicht erreichbar – Luftlinie');
    return stubRoute(from, to);
  }
}
