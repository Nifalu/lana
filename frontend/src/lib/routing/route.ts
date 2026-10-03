import { showToast } from '../hud/toasts.svelte';
import type { LngLat } from '../state/app.svelte';
import { postJson } from './api';

export type Route = {
  line: GeoJSON.LineString;
  distanceKm: number;
  durationS: number;
  /** `stub` is a straight line, not a real route. */
  source: 'api' | 'stub';
};

/** Pedestrian speed used for the straight-line stub. */
const WALKING_KMH = 5;

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

/** The route Feature the API returns (also carried inside `/route_to_closest_cooling`). */
export type ApiRouteFeature = {
  properties: { length_km: number; time_seconds: number };
  geometry: GeoJSON.LineString;
};

/** Validate an API route Feature and turn it into a `Route`. Throws on a malformed one. */
export function routeFromFeature(feature: ApiRouteFeature): Route {
  const coordinates = feature?.geometry?.coordinates;
  const { length_km, time_seconds } = feature?.properties ?? {};
  if (!Array.isArray(coordinates) || coordinates.length < 2) throw new Error('routing: empty shape');
  if (!Number.isFinite(length_km) || !Number.isFinite(time_seconds)) {
    throw new Error('routing: response has no summary');
  }
  return {
    line: { type: 'LineString', coordinates: coordinates.map(([lon, lat]) => [lon, lat]) },
    distanceKm: length_km,
    durationS: time_seconds,
    source: 'api',
  };
}

/**
 * Walking route from A to B, from the lana API (`POST /calculate_route`).
 *
 * A failed request (network, CORS, 4xx/5xx, timeout) falls back to the
 * straight-line stub with a toast, so the UI keeps working. Only an abort
 * by the caller rejects.
 */
export async function route(from: LngLat, to: LngLat, signal?: AbortSignal): Promise<Route> {
  try {
    const feature = await postJson<ApiRouteFeature>(
      '/calculate_route',
      {
        start: { longitude: from[0], latitude: from[1] },
        end: { longitude: to[0], latitude: to[1] },
      },
      signal,
    );
    return routeFromFeature(feature);
  } catch (err) {
    if (signal?.aborted) throw err;
    console.warn('routing failed, using a straight line', err);
    showToast('Routing nicht erreichbar – Luftlinie');
    return stubRoute(from, to);
  }
}
