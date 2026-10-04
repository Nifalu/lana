import type { Station } from '../data/types';

/**
 * Live temperature surface interpolated from the air sensors.
 *
 * Every grid cell gets the inverse-distance-weighted mean of the fresh
 * sensor readings, so colour means temperature (not sensor density).
 * Cells far from any sensor fade out instead of showing a guess.
 */

/** Area covered by the surface (Kanton Basel-Stadt plus a margin). */
export const TEMP_BOUNDS = { west: 7.5, east: 7.72, south: 47.5, north: 47.62 } as const;

/** Fixed colour scale, °C → RGB. Shared by the surface, the sensor dots and the legend. */
export const TEMP_STOPS: readonly [number, readonly [number, number, number]][] = [
  [16, [59, 130, 246]],
  [20, [45, 190, 150]],
  [24, [250, 204, 21]],
  [28, [249, 115, 22]],
  [32, [220, 38, 38]],
  [36, [127, 29, 29]],
];

const CELL_M = 60; // grid resolution
const FULL_M = 700; // fully opaque up to this distance from the nearest sensor
const FADE_M = 1500; // fully transparent beyond this
const MAX_AGE_MS = 90 * 60_000; // readings older than this (vs. the newest) are ignored
const OUTLIER_K = 3.5; // robust z-score cut-off (median / MAD)

export type Reading = { id: string; lon: number; lat: number; t: number };

/** Fresh, plausible readings: recent relative to the newest one, outliers removed. */
export function freshReadings(stations: Station[]): Reading[] {
  const withT = stations.filter(
    (s): s is Station & { temperature_c: number } => typeof s.temperature_c === 'number',
  );
  const times = withT.map((s) => (s.measured_at ? Date.parse(s.measured_at) : NaN));
  const newest = Math.max(...times.filter(Number.isFinite));
  const recent = withT.filter((_, i) =>
    Number.isFinite(newest) ? Number.isFinite(times[i]) && newest - times[i] <= MAX_AGE_MS : true,
  );
  if (recent.length < 3) return recent.map(toReading);

  const ts = recent.map((s) => s.temperature_c);
  const med = median(ts);
  const mad = median(ts.map((t) => Math.abs(t - med))) || 0.5;
  return recent
    .filter((s) => Math.abs(s.temperature_c - med) / (1.4826 * mad) <= OUTLIER_K)
    .map(toReading);
}

function toReading(s: Station & { temperature_c: number }): Reading {
  return { id: s.id, lon: s.lon, lat: s.lat, t: s.temperature_c };
}

function median(xs: number[]): number {
  const a = [...xs].sort((x, y) => x - y);
  const m = a.length >> 1;
  return a.length % 2 ? a[m] : (a[m - 1] + a[m]) / 2;
}

/** RGB for a temperature on the fixed scale (clamped at both ends). */
export function colorFor(t: number): [number, number, number] {
  if (t <= TEMP_STOPS[0][0]) return [...TEMP_STOPS[0][1]];
  for (let i = 1; i < TEMP_STOPS.length; i++) {
    const [t1, c1] = TEMP_STOPS[i];
    if (t <= t1) {
      const [t0, c0] = TEMP_STOPS[i - 1];
      const f = (t - t0) / (t1 - t0);
      return [0, 1, 2].map((k) => Math.round(c0[k] + f * (c1[k] - c0[k]))) as [number, number, number];
    }
  }
  return [...TEMP_STOPS[TEMP_STOPS.length - 1][1]];
}

/** Corner coordinates for a MapLibre image source: TL, TR, BR, BL. */
export const TEMP_COORDINATES: [[number, number], [number, number], [number, number], [number, number]] = [
  [TEMP_BOUNDS.west, TEMP_BOUNDS.north],
  [TEMP_BOUNDS.east, TEMP_BOUNDS.north],
  [TEMP_BOUNDS.east, TEMP_BOUNDS.south],
  [TEMP_BOUNDS.west, TEMP_BOUNDS.south],
];

/** A transparent 1×1 PNG, used while there are no readings. */
export const EMPTY_IMAGE =
  'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII=';

/** Render the interpolated surface to a PNG data URL (EMPTY_IMAGE if no readings). */
export function renderTemperature(readings: Reading[]): string {
  if (readings.length === 0) return EMPTY_IMAGE;

  const { west, east, south, north } = TEMP_BOUNDS;
  const lat0 = ((south + north) / 2) * (Math.PI / 180);
  const mPerDegLon = 111_320 * Math.cos(lat0);
  const mPerDegLat = 110_574;
  const width = Math.round(((east - west) * mPerDegLon) / CELL_M);
  const height = Math.round(((north - south) * mPerDegLat) / CELL_M);

  // Sensor positions in metres relative to the north-west corner.
  const sx = readings.map((r) => (r.lon - west) * mPerDegLon);
  const sy = readings.map((r) => (north - r.lat) * mPerDegLat);
  const st = readings.map((r) => r.t);

  const canvas = document.createElement('canvas');
  canvas.width = width;
  canvas.height = height;
  const ctx = canvas.getContext('2d');
  if (!ctx) return EMPTY_IMAGE;
  const img = ctx.createImageData(width, height);

  for (let j = 0; j < height; j++) {
    const y = (j + 0.5) * CELL_M;
    for (let i = 0; i < width; i++) {
      const x = (i + 0.5) * CELL_M;
      let wSum = 0;
      let tSum = 0;
      let nearest2 = Infinity;
      for (let k = 0; k < st.length; k++) {
        const dx = x - sx[k];
        const dy = y - sy[k];
        const d2 = dx * dx + dy * dy;
        if (d2 < nearest2) nearest2 = d2;
        const w = 1 / Math.max(d2, 2500); // power 2, distance floored at 50 m
        wSum += w;
        tSum += w * st[k];
      }
      const nearest = Math.sqrt(nearest2);
      const alpha = nearest <= FULL_M ? 1 : nearest >= FADE_M ? 0 : 1 - (nearest - FULL_M) / (FADE_M - FULL_M);
      const [r, g, b] = colorFor(tSum / wSum);
      const p = (j * width + i) * 4;
      img.data[p] = r;
      img.data[p + 1] = g;
      img.data[p + 2] = b;
      img.data[p + 3] = Math.round(alpha * 255);
    }
  }
  ctx.putImageData(img, 0, 0);
  return canvas.toDataURL('image/png');
}
