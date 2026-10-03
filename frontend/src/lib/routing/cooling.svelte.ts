import { data } from '../data/store.svelte';
import type { Poi } from '../data/types';
import { showToast } from '../hud/toasts.svelte';
import type { LngLat } from '../state/app.svelte';
import { selection, type Selection } from '../state/selection.svelte';
import { postJson } from './api';
import { haversineKm, routeFromFeature, stubRoute, type ApiRouteFeature, type Route } from './route';
import { beginRequest, requireOrigin, routing } from './routing.svelte';

/** What the "nearest cool spot" button looks for. */
export type CoolFilter = 'all' | 'fountain' | 'cool_place';
type CoolKind = Exclude<CoolFilter, 'all'>;

export const COOL_FILTERS: { value: CoolFilter; label: string }[] = [
  { value: 'all', label: 'Alles' },
  { value: 'fountain', label: 'Trinkbrunnen' },
  { value: 'cool_place', label: 'Kühle Orte' },
];

const STORAGE_KEY = 'lana.coolFilter';

function loadFilter(): CoolFilter {
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (COOL_FILTERS.some((f) => f.value === stored)) return stored as CoolFilter;
  } catch {
    // Storage can be blocked; the default is fine.
  }
  return 'all';
}

export const cooling = $state({ filter: loadFilter() });

export function setCoolFilter(filter: CoolFilter): void {
  cooling.filter = filter;
  try {
    localStorage.setItem(STORAGE_KEY, filter);
  } catch {
    // Not remembered, still applied.
  }
}

/** The API's `filter` body field: null means everything. */
const apiFilter = (filter: CoolFilter): CoolKind[] | null => (filter === 'all' ? null : [filter]);

const kindsOf = (filter: CoolFilter): CoolKind[] =>
  filter === 'all' ? ['fountain', 'cool_place'] : [filter];

const KIND_LABELS: Record<CoolKind, string> = {
  fountain: 'Trinkbrunnen',
  cool_place: 'Kühler Ort',
};
/** Shown when the kind of a target cannot be told. */
const UNKNOWN_LABEL = 'Kühler Ort / Brunnen';

/** Max distance between the API's target and one of our POIs to call them the same place. */
const MATCH_M = 30;

/** Nearest loaded POI of the wanted kinds, with its distance in metres. */
function nearestPoi(to: LngLat, kinds: CoolKind[]): { poi: Poi; metres: number } | null {
  let best: { poi: Poi; metres: number } | null = null;
  for (const poi of data.pois) {
    if (!kinds.includes(poi.kind as CoolKind)) continue;
    const metres = haversineKm(to, [poi.lon, poi.lat]) * 1000;
    if (!best || metres < best.metres) best = { poi, metres };
  }
  return best;
}

/** The sheet's subject for a cool spot we have routed to. */
function targetSelection(coords: LngLat, name: string | null, kind: CoolKind | null, poi?: Poi): Selection {
  return {
    coords,
    name: name ?? poi?.name ?? null,
    kind: kind ?? 'unknown',
    label: kind ? KIND_LABELS[kind] : UNKNOWN_LABEL,
    source: 'lana',
    properties: { ...poi?.properties, kind },
  };
}

type CoolingFeature = ApiRouteFeature & {
  name?: string | null;
  longitude: number;
  latitude: number;
};

/**
 * What kind the returned target is. The API overwrites the target's own
 * `type` with "Feature", so the kind is recovered from our loaded POIs (a
 * match within 30 m), else from the filter when it names a single kind.
 */
function recoverKind(coords: LngLat, filter: CoolFilter): { kind: CoolKind | null; poi?: Poi } {
  const near = nearestPoi(coords, kindsOf(filter));
  if (near && near.metres <= MATCH_M) return { kind: near.poi.kind as CoolKind, poi: near.poi };
  return { kind: filter === 'all' ? null : filter };
}

function show(result: Route, target: Selection): void {
  routing.current = result;
  routing.target = target;
  routing.status = 'idle';
  // The sheet then describes the target and offers "Route beenden".
  selection.current = target;
}

/**
 * Route from the user's position to the nearest cool spot of the chosen
 * kind, via `POST /route_to_closest_cooling`. Offline, falls back to a
 * straight line to the nearest known spot.
 */
export async function startNearestCooling(): Promise<void> {
  if (routing.status === 'loading') return;
  const signal = beginRequest();
  const origin = await requireOrigin(signal);
  if (!origin) return;

  const filter = cooling.filter;
  try {
    const features = await postJson<CoolingFeature[]>(
      '/route_to_closest_cooling?number_results=1',
      {
        location: { longitude: origin[0], latitude: origin[1] },
        filter: apiFilter(filter),
      },
      signal,
    );
    if (signal.aborted) return;
    const first = features[0];
    if (!first) throw new Error('no cooling location returned');
    const coords: LngLat = [first.longitude, first.latitude];
    if (!coords.every(Number.isFinite)) throw new Error('cooling target has no position');
    const { kind, poi } = recoverKind(coords, filter);
    show(routeFromFeature(first), targetSelection(coords, first.name?.trim() || null, kind, poi));
  } catch (err) {
    if (signal.aborted) return;
    console.warn('nearest cool spot failed', err);
    const fallback = nearestPoi(origin, kindsOf(filter));
    if (!fallback) {
      routing.status = 'idle';
      showToast('Kühler Ort nicht erreichbar');
      return;
    }
    const { poi } = fallback;
    const coords: LngLat = [poi.lon, poi.lat];
    show(stubRoute(origin, coords), targetSelection(coords, null, poi.kind as CoolKind, poi));
    showToast('Kühler Ort nicht erreichbar – Luftlinie zum nächsten');
  }
}
