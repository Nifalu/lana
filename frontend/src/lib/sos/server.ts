import { invoke, isTauri } from '@tauri-apps/api/core';
import { getServerBaseUrl } from '../data';
import type { LngLat } from '../state/app.svelte';

/** A point as the middleware spells it. */
export type LonLat = { lon: number; lat: number };

export type HelpStatus = 'open' | 'responded' | 'resolved' | 'cancelled';

export type HelpRequest = {
  id: string;
  status: HelpStatus;
  note: string | null;
  location: LonLat;
  radius_m: number;
  created_at: string;
  updated_at: string;
};

export const toLonLat = ([lon, lat]: LngLat): LonLat => ({ lon, lat });
export const fromLonLat = (p: LonLat): LngLat => [p.lon, p.lat];

/** The middleware answered with a non-2xx status. */
export class ApiError extends Error {
  readonly status: number;
  constructor(status: number, message: string) {
    super(message);
    this.name = 'ApiError';
    this.status = status;
  }
}

// --- identity -------------------------------------------------------------

const DEVICE_KEY = 'lana-device-id';
let deviceId: Promise<string> | null = null;

/** Anonymous, persistent id of this install. */
export function getDeviceId(): Promise<string> {
  deviceId ??= loadDeviceId();
  return deviceId;
}

async function loadDeviceId(): Promise<string> {
  if (isTauri()) return invoke<string>('get_device_id');
  try {
    const stored = localStorage.getItem(DEVICE_KEY);
    if (stored) return stored;
  } catch {
    // Storage blocked: fall through to an id that lives as long as the page.
  }
  const id = crypto.randomUUID();
  try {
    localStorage.setItem(DEVICE_KEY, id);
  } catch {
    // Same: in-memory id.
  }
  return id;
}

let baseUrl: Promise<string> | null = null;

/** Base URL of the middleware, resolved once. */
export function getBaseUrl(): Promise<string> {
  baseUrl ??= getServerBaseUrl();
  return baseUrl;
}

// --- REST -----------------------------------------------------------------

const TIMEOUT_MS = 10_000;

async function call<T>(method: string, path: string, body?: unknown): Promise<T> {
  const url = `${await getBaseUrl()}/api/v1${path}`;
  const res = await fetch(url, {
    method,
    headers: body === undefined ? undefined : { 'content-type': 'application/json' },
    body: body === undefined ? undefined : JSON.stringify(body),
    signal: AbortSignal.timeout(TIMEOUT_MS),
  });
  if (!res.ok) {
    let message = res.statusText;
    try {
      const data = await res.json();
      if (typeof data?.error === 'string') message = data.error;
    } catch {
      // Not JSON: keep the status text.
    }
    throw new ApiError(res.status, message);
  }
  return (await res.json()) as T;
}

export async function putDevice(isHelper: boolean, location: LngLat | null): Promise<void> {
  const id = await getDeviceId();
  await call('PUT', `/devices/${id}`, {
    is_helper: isHelper,
    location: location ? toLonLat(location) : null,
  });
}

export async function createHelpRequest(
  location: LngLat,
  note: string | undefined,
): Promise<HelpRequest> {
  return call('POST', '/help-requests', {
    device_id: await getDeviceId(),
    location: toLonLat(location),
    ...(note ? { note } : {}),
  });
}

export async function helpRequestAction(
  id: string,
  action: 'respond' | 'resolve' | 'cancel',
): Promise<HelpRequest> {
  return call('POST', `/help-requests/${id}/${action}`, { device_id: await getDeviceId() });
}

/**
 * Requests within `radiusM` of `near`; `status` omitted = any.
 * `excludeOwn` asks the server to leave out this device's own requests
 * (servers without that filter ignore the parameter).
 */
export async function listHelpRequests(
  near: LngLat,
  radiusM: number,
  status?: HelpStatus,
  excludeOwn = false,
): Promise<HelpRequest[]> {
  const q = new URLSearchParams({ near: `${near[0]},${near[1]}`, radius_m: String(radiusM) });
  if (status) q.set('status', status);
  if (excludeOwn) q.set('exclude_device_id', await getDeviceId());
  return call('GET', `/help-requests?${q}`);
}

/** Opens the SSE stream. Caller closes it. */
export async function openEvents(): Promise<EventSource> {
  const id = await getDeviceId();
  return new EventSource(`${await getBaseUrl()}/api/v1/events?device_id=${id}`);
}
