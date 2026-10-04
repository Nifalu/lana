import type { Geometry, Point } from 'geojson';
import type { Map, MapGeoJSONFeature, MapMouseEvent, PointLike } from 'maplibre-gl';
import { location, type LngLat } from '../state/app.svelte';
import { selection, type Selection } from '../state/selection.svelte';
import { LAYER } from './layers';

/**
 * Tap handling: picking, building a `Selection`, hover cursor.
 *
 * Order matters: `queryRenderedFeatures` returns the topmost feature first.
 */
export const INTERACTIVE_LAYERS = [
  LAYER.water,
  LAYER.cool,
  LAYER.stations,
  LAYER.swim,
  'poi_rank1',
  'poi_rank2',
] as const;

/** Half-size in px of the box around a tap, sized for a fingertip. */
const TAP_RADIUS = 12;
/** Smaller box for the desktop hover cursor. */
const HOVER_RADIUS = 5;
/** Our own markers win over a swisstopo icon at nearly the same spot. */
const OURS_BONUS_PX = 4;
/** Below this zoom the station circles are still fully transparent. */
const STATIONS_MIN_ZOOM = 13;

const isOurs = (layerId: string) => layerId.startsWith('lana-');

// ---------------------------------------------------------------------------
// Labels
// ---------------------------------------------------------------------------

const OUR_LABELS: Record<string, string> = {
  fountain: 'Trinkbrunnen',
  swim_area: 'Rheinschwimmen / Badestelle',
  cool_place: 'Kühler Ort',
  air: 'Temperatur-Messstation',
};

/** swisstopo `subclass` values, checked before `class`. */
const SWISSTOPO_LABELS: Record<string, string> = {
  school: 'Schule',
  kindergarten: 'Kindergarten',
  college: 'Hochschule',
  university: 'Hochschule',
  hospital: 'Spital',
  clinic: 'Klinik',
  doctors: 'Arztpraxis',
  pharmacy: 'Apotheke',
  place_of_worship: 'Kirche',
  church: 'Kirche',
  chapel: 'Kapelle',
  cathedral: 'Kirche',
  mosque: 'Moschee',
  synagogue: 'Synagoge',
  monastery: 'Kloster',
  government: 'Verwaltung',
  townhall: 'Rathaus',
  courthouse: 'Gericht',
  police: 'Polizei',
  fire_station: 'Feuerwehr',
  post_office: 'Post',
  library: 'Bibliothek',
  theatre: 'Theater',
  cinema: 'Kino',
  museum: 'Museum',
  gallery: 'Galerie',
  attraction: 'Sehenswürdigkeit',
  viewpoint: 'Aussichtspunkt',
  monument: 'Denkmal',
  memorial: 'Denkmal',
  castle: 'Schloss',
  archaeological_site: 'Sehenswürdigkeit',
  ruins: 'Ruine',
  park: 'Park',
  garden: 'Garten',
  playground: 'Spielplatz',
  zoo: 'Zoo',
  cemetery: 'Friedhof',
  swimming_pool: 'Schwimmbad',
  swimming: 'Schwimmbad',
  sports_centre: 'Sportanlage',
  stadium: 'Stadion',
  pitch: 'Sportplatz',
  fitness_centre: 'Fitnesscenter',
  restaurant: 'Restaurant',
  cafe: 'Café',
  fast_food: 'Imbiss',
  bar: 'Bar',
  pub: 'Bar',
  hotel: 'Hotel',
  supermarket: 'Supermarkt',
  marketplace: 'Markt',
  toilets: 'WC',
  drinking_water: 'Trinkwasser',
  fountain: 'Brunnen',
};

/** swisstopo `class` fallbacks when the subclass is unknown. */
const SWISSTOPO_CLASS_LABELS: Record<string, string> = {
  school: 'Schule',
  college: 'Hochschule',
  hospital: 'Spital',
  place_of_worship: 'Kirche',
  monastery: 'Kloster',
  government: 'Verwaltung',
  attraction: 'Sehenswürdigkeit',
  historic: 'Sehenswürdigkeit',
  museum: 'Museum',
  park: 'Park',
  tower: 'Turm',
  zoo: 'Zoo',
  cemetery: 'Friedhof',
  stadium: 'Sportanlage',
  swimming: 'Schwimmbad',
  library: 'Bibliothek',
  theatre: 'Theater',
  cinema: 'Kino',
  restaurant: 'Restaurant',
  cafe: 'Café',
  shop: 'Geschäft',
  lodging: 'Unterkunft',
};

export function swisstopoLabel(subclass: unknown, cls: unknown): string {
  const sub = typeof subclass === 'string' ? subclass : '';
  const base = typeof cls === 'string' ? cls : '';
  if (SWISSTOPO_LABELS[sub]) return SWISSTOPO_LABELS[sub];
  if (sub.startsWith('historic')) return 'Sehenswürdigkeit';
  if (SWISSTOPO_CLASS_LABELS[base]) return SWISSTOPO_CLASS_LABELS[base];
  if (base.startsWith('historic')) return 'Sehenswürdigkeit';
  return 'Ort';
}

// ---------------------------------------------------------------------------
// Feature -> Selection
// ---------------------------------------------------------------------------

const text = (value: unknown): string | null =>
  typeof value === 'string' && value.trim() ? value.trim() : null;

const isFiniteNumber = (value: unknown): value is number =>
  typeof value === 'number' && Number.isFinite(value);

/** Mean of the outer ring's vertices (without the repeated closing point). */
function ringCentroid(ring: number[][]): LngLat | null {
  const points = ring.length > 1 ? ring.slice(0, -1) : ring;
  if (!points.length) return null;
  let lon = 0;
  let lat = 0;
  for (const [x, y] of points) {
    lon += x;
    lat += y;
  }
  return [lon / points.length, lat / points.length];
}

/** Ray casting test, in plain lon/lat (fine at the scale of one polygon). */
function inRing([x, y]: LngLat, ring: number[][]): boolean {
  let inside = false;
  for (let i = 0, j = ring.length - 1; i < ring.length; j = i++) {
    const [xi, yi] = ring[i];
    const [xj, yj] = ring[j];
    if (yi > y !== yj > y && x < ((xj - xi) * (y - yi)) / (yj - yi) + xi) inside = !inside;
  }
  return inside;
}

/** The ring vertex closest to `to`, with longitude scaled to metres-ish. */
function nearestVertex(to: LngLat, rings: number[][][]): LngLat | null {
  const k = Math.cos((to[1] * Math.PI) / 180);
  let best: LngLat | null = null;
  let bestD = Infinity;
  for (const ring of rings) {
    for (const [x, y] of ring) {
      const d = ((x - to[0]) * k) ** 2 + (y - to[1]) ** 2;
      if (d < bestD) {
        bestD = d;
        best = [x, y];
      }
    }
  }
  return best;
}

/**
 * The point a feature is selected and routed by.
 *  - Point: its own position.
 *  - Polygon: the `lon`/`lat` property if that lies on the polygon (for a
 *    long, curved stretch of river it can fall on the bank instead), else
 *    the tapped spot if that is on the polygon, else the nearest vertex to
 *    the tap, else the centroid of the outer ring.
 */
export function representativePoint(
  geometry: Geometry,
  properties: Record<string, unknown> | null,
  near?: LngLat,
): LngLat | null {
  switch (geometry.type) {
    case 'Point':
      return [geometry.coordinates[0], geometry.coordinates[1]];
    case 'Polygon':
    case 'MultiPolygon': {
      const rings =
        geometry.type === 'Polygon'
          ? [geometry.coordinates[0]]
          : geometry.coordinates.map((polygon) => polygon[0]);
      const given: LngLat | null =
        isFiniteNumber(properties?.lon) && isFiniteNumber(properties?.lat)
          ? [properties.lon, properties.lat]
          : null;
      if (given && rings.some((ring) => inRing(given, ring))) return given;
      if (near) {
        if (rings.some((ring) => inRing(near, ring))) return near;
        const vertex = nearestVertex(near, rings);
        if (vertex) return vertex;
      }
      return given ?? (rings[0] ? ringCentroid(rings[0]) : null);
    }
    default:
      return null;
  }
}

/**
 * Build a `Selection` from a rendered feature. Pure: `layerId` tells ours
 * (`lana-*`) from swisstopo, `fallback` is the tapped position: it steers
 * the choice of point on a polygon and stands in when there is no geometry. Returns null when no position can be
 * determined.
 */
export function selectionFromFeature(
  feature: { geometry: Geometry; properties: Record<string, unknown> | null },
  layerId: string,
  fallback?: LngLat,
): Selection | null {
  const properties = { ...(feature.properties ?? {}) };
  const coords = representativePoint(feature.geometry, properties, fallback) ?? fallback ?? null;
  if (!coords) return null;

  if (isOurs(layerId)) {
    const kind = text(properties.kind) ?? 'unknown';
    return {
      coords,
      name: text(properties.name),
      kind,
      label: OUR_LABELS[kind] ?? 'Ort',
      source: 'lana',
      properties,
    };
  }

  const subclass = text(properties.subclass);
  const cls = text(properties.class);
  return {
    coords,
    name: text(properties['name:latin']) ?? text(properties.name),
    kind: subclass ?? cls ?? 'unknown',
    label: swisstopoLabel(subclass, cls),
    source: 'swisstopo',
    properties,
  };
}

// ---------------------------------------------------------------------------
// Picking
// ---------------------------------------------------------------------------

function featureAt(map: Map, x: number, y: number, radius: number): MapGeoJSONFeature | null {
  const layers = INTERACTIVE_LAYERS.filter((id) => map.getLayer(id));
  if (!layers.length) return null;
  const box: [PointLike, PointLike] = [
    [x - radius, y - radius],
    [x + radius, y + radius],
  ];
  const zoom = map.getZoom();
  const hits = map
    .queryRenderedFeatures(box, { layers })
    .filter((f) => f.layer.id !== LAYER.stations || zoom >= STATIONS_MIN_ZOOM);

  // Points beat the swim polygon underneath them; among points the nearest
  // to the tap wins, our own markers with a small head start.
  const points = hits.filter((f) => f.geometry.type === 'Point');
  if (points.length) {
    const score = (f: MapGeoJSONFeature) => {
      const [lon, lat] = (f.geometry as Point).coordinates;
      const p = map.project([lon, lat]);
      return Math.hypot(p.x - x, p.y - y) - (isOurs(f.layer.id) ? OURS_BONUS_PX : 0);
    };
    // sort is stable, so equal scores keep the topmost-first order.
    return [...points].sort((a, b) => score(a) - score(b))[0];
  }
  return hits[0] ?? null;
}

/** Selection for what is under a screen position, or null. */
export function pickSelection(map: Map, x: number, y: number, radius = TAP_RADIUS): Selection | null {
  const feature = featureAt(map, x, y, radius);
  if (!feature) return null;
  const tap = map.unproject([x, y]);
  return selectionFromFeature(feature, feature.layer.id, [tap.lng, tap.lat]);
}

// ---------------------------------------------------------------------------
// Map handlers
// ---------------------------------------------------------------------------

/** The one click handler of the map. */
export function handleMapClick(map: Map, e: MapMouseEvent): void {
  // Debug: while `location.picking`, the next tap sets the position.
  if (location.picking) {
    location.coords = [e.lngLat.lng, e.lngLat.lat];
    location.accuracyM = null;
    location.source = 'manual';
    location.picking = false;
    return;
  }
  selection.current = pickSelection(map, e.point.x, e.point.y);
}

/** Pointer cursor over tappable things. Leaves the crosshair alone while picking. */
export function handleMapHover(map: Map, e: MapMouseEvent): void {
  if (location.picking) return;
  const hit = featureAt(map, e.point.x, e.point.y, HOVER_RADIUS);
  map.getCanvas().style.cursor = hit ? 'pointer' : '';
}

/** Wire click and hover onto a map. */
export function bindInteraction(map: Map): void {
  map.on('click', (e) => handleMapClick(map, e));
  map.on('mousemove', (e) => handleMapHover(map, e));
}
