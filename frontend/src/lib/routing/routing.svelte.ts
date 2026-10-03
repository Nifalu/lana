import { showToast } from '../hud/toasts.svelte';
import { location, mapActions } from '../state/app.svelte';
import type { Selection } from '../state/selection.svelte';
import { route, type Route } from './route';

/**
 * Route shown on the map.
 *  - `current`: the route being displayed.
 *  - `target`: what it leads to, kept so the sheet can describe the route
 *    after the selection has moved on.
 *  - `status`: state of the latest request.
 */
export const routing = $state({
  current: null as Route | null,
  status: 'idle' as 'idle' | 'loading' | 'error',
  target: null as Selection | null,
});

// Only one request is ever in flight; a new one or `endRoute` aborts it.
let inflight: AbortController | null = null;

/** Route from the user's position to `target`, locating first if needed. */
export async function startRoute(target: Selection): Promise<void> {
  inflight?.abort();
  const controller = new AbortController();
  inflight = controller;
  const { signal } = controller;
  routing.status = 'loading';

  if (!location.coords) {
    await mapActions.locate();
    if (signal.aborted) return;
  }
  const origin = location.coords;
  if (!origin) {
    routing.status = 'idle';
    showToast('Zuerst Standort bestimmen', {
      action: { label: 'Auf Karte wählen', run: () => (location.picking = true) },
    });
    return;
  }

  try {
    const result = await route(origin, target.coords, signal);
    if (signal.aborted) return;
    routing.current = result;
    routing.target = target;
    routing.status = 'idle';
  } catch (err) {
    if (signal.aborted) return;
    console.error('route failed', err);
    routing.status = 'error';
    showToast('Route konnte nicht berechnet werden');
  }
}

/** Remove the route and cancel any request still running. */
export function endRoute(): void {
  inflight?.abort();
  inflight = null;
  routing.current = null;
  routing.target = null;
  routing.status = 'idle';
}
