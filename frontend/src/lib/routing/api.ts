/** Used when `VITE_API_URL` is not set. */
const DEFAULT_API_URL = 'https://lana.heitzli.ch';

export const REQUEST_TIMEOUT_MS = 10_000;

/** Full URL of an endpoint of the lana API, e.g. `apiUrl('/calculate_route')`. */
export function apiUrl(path: string): string {
  const base = (import.meta.env.VITE_API_URL?.trim() || DEFAULT_API_URL).replace(/\/+$/, '');
  return `${base}${path.startsWith('/') ? '' : '/'}${path}`;
}

/**
 * POST a JSON body and parse the JSON answer. Rejects on network errors,
 * non-2xx statuses and after `REQUEST_TIMEOUT_MS`; `signal` aborts earlier.
 */
export async function postJson<T>(path: string, body: unknown, signal?: AbortSignal): Promise<T> {
  const timeout = AbortSignal.timeout(REQUEST_TIMEOUT_MS);
  const response = await fetch(apiUrl(path), {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
    signal: signal ? AbortSignal.any([signal, timeout]) : timeout,
  });
  if (!response.ok) throw new Error(`${path}: HTTP ${response.status} ${response.statusText}`);
  return (await response.json()) as T;
}
