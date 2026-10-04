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

/** One cool spot offered by the button: the route there and what it leads to. */
export type CoolCandidate = { route: Route; target: Selection };

/** How many cool spots one tap fetches, to browse through in the sheet. */
const COOL_RESULTS = 5;

export const cooling = $state({
  filter: loadFilter(),
  /** The spots from the last tap, nearest walk first. */
  candidates: [] as CoolCandidate[],
  /** Which candidate is on the map. */
  index: 0,
});

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

/** The `count` nearest loaded POIs of the wanted kinds, with their distance in metres. */
function nearestPois(to: LngLat, kinds: CoolKind[], count: number): { poi: Poi; metres: number }[] {
  return data.pois
    .filter((poi) => kinds.includes(poi.kind as CoolKind))
    .map((poi) => ({ poi, metres: haversineKm(to, [poi.lon, poi.lat]) * 1000 }))
    .sort((a, b) => a.metres - b.metres)
    .slice(0, count);
}

function nearestPoi(to: LngLat, kinds: CoolKind[]): { poi: Poi; metres: number } | null {
  return nearestPois(to, kinds, 1)[0] ?? null;
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

/** Put candidate `index` (clamped) on the map and in the sheet. */
export function showCoolCandidate(index: number): void {
  const list = cooling.candidates;
  if (list.length === 0) return;
  const i = Math.max(0, Math.min(list.length - 1, index));
  cooling.index = i;
  show(list[i].route, list[i].target);
}

/** True while the route on the map is one of the button's candidates. */
export function isCoolCandidateShown(target: Selection | null): boolean {
  const current = cooling.candidates[cooling.index];
  return (
    !!current &&
    !!target &&
    current.target.coords[0] === target.coords[0] &&
    current.target.coords[1] === target.coords[1]
  );
}

function offer(candidates: CoolCandidate[]): void {
  cooling.candidates = candidates;
  showCoolCandidate(0);
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
      `/route_to_closest_cooling?number_results=${COOL_RESULTS}`,
      {
        location: { longitude: origin[0], latitude: origin[1] },
        filter: apiFilter(filter),
      },
      signal,
    );
    if (signal.aborted) return;
    const candidates: CoolCandidate[] = [];
    for (const feature of features) {
      const coords: LngLat = [feature.longitude, feature.latitude];
      if (!coords.every(Number.isFinite)) continue;
      const { kind, poi } = recoverKind(coords, filter);
      candidates.push({
        route: routeFromFeature(feature),
        target: targetSelection(coords, feature.name?.trim() || null, kind, poi),
      });
    }
    if (candidates.length === 0) throw new Error('no cooling location returned');
    // The API ranks by straight-line distance; offer the shortest walk first.
    candidates.sort((a, b) => a.route.distanceKm - b.route.distanceKm);
    offer(candidates);
  } catch (err) {
    if (signal.aborted) return;
    console.warn('nearest cool spot failed', err);
    const fallback = nearestPois(origin, kindsOf(filter), COOL_RESULTS);
    if (fallback.length === 0) {
      routing.status = 'idle';
      showToast('Kühler Ort nicht erreichbar');
      return;
    }
    offer(
      fallback.map(({ poi }) => {
        const coords: LngLat = [poi.lon, poi.lat];
        return {
          route: stubRoute(origin, coords),
          target: targetSelection(coords, null, poi.kind as CoolKind, poi),
        };
      }),
    );
    showToast('Kühler Ort nicht erreichbar – Luftlinie zum nächsten');
  }
}
