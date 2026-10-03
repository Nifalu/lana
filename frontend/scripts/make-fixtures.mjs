#!/usr/bin/env node
// Generates the static fixture data the app shows when no lana server is
// reachable (browser builds, Tauri before the first successful sync).
//
//   node scripts/make-fixtures.mjs
//
// Output: src/lib/data/fixtures/{fountains,swim_areas,cool_places,stations,meta}.json
// The Poi / Station shapes mirror src-tauri/src/cache.rs, and the name/id
// mapping mirrors server/src/import.rs and server/src/poller.rs.

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = join(here, '..', '..');
const outDir = join(here, '..', 'src', 'lib', 'data', 'fixtures');

const ODS = 'https://data.bs.ch/api/explore/v2.1';
const FOUNTAINS_URL =
  'https://raw.githubusercontent.com/MarkusCouch/MunchiesMaps/main/resources/geojson/switzerland/basel-stadt/drinking_water.geojson';

/** Rough bounding box around Basel-Stadt, used to drop strays. */
const BASEL_BBOX = { minLon: 7.45, minLat: 47.5, maxLon: 7.75, maxLat: 47.62 };
/** Air readings older than this (relative to the newest one) count as offline stations. */
const MAX_READING_AGE_MS = 6 * 60 * 60 * 1000;
const AIR_PAGE_SIZE = 100;
const AIR_PAGE_CAP = 15;

async function fetchJson(url) {
  const res = await fetch(url, { headers: { 'user-agent': 'lana-fixtures' } });
  if (!res.ok) throw new Error(`${url}: HTTP ${res.status}`);
  return res.json();
}

async function readJson(path) {
  return JSON.parse(await readFile(path, 'utf8'));
}

const inBasel = (lon, lat) =>
  lon >= BASEL_BBOX.minLon && lon <= BASEL_BBOX.maxLon && lat >= BASEL_BBOX.minLat && lat <= BASEL_BBOX.maxLat;

// ---- fountains (OpenStreetMap via MunchiesMaps) -----------------------------

async function makeFountains() {
  const fc = await fetchJson(FOUNTAINS_URL);
  return fc.features.map((f) => {
    const { id, osm_type, name, ...tags } = f.properties;
    const [lon, lat] = f.geometry.coordinates;
    return {
      id,
      kind: 'fountain',
      name: name ?? 'Trinkwasser',
      source: 'osm',
      source_id: `${osm_type ?? 'node'}/${id}`,
      lon,
      lat,
      geometry: { type: 'Point', coordinates: [lon, lat] },
      properties: tags,
    };
  });
}

// ---- swim areas (data.bs.ch 100270) -----------------------------------------

function ringCentroid(ring) {
  const n = ring.length;
  return [ring.reduce((s, p) => s + p[0], 0) / n, ring.reduce((s, p) => s + p[1], 0) / n];
}

async function makeSwimAreas() {
  let fc;
  try {
    fc = await fetchJson(`${ODS}/catalog/datasets/100270/exports/geojson`);
  } catch (err) {
    console.warn(`swim areas: live export failed (${err.message}), using server/testdata`);
    fc = await readJson(join(repoRoot, 'server', 'testdata', 'swim-areas.geojson'));
  }
  const areas = fc.features.map((f) => {
    const gp = f.properties?.geo_point_2d;
    const [lon, lat] = gp ? [gp.lon, gp.lat] : ringCentroid(f.geometry.coordinates[0]);
    return { lon, lat, geometry: f.geometry };
  });
  // Same deterministic numbering as import.rs: order by centroid.
  areas.sort((a, b) => a.lon - b.lon || a.lat - b.lat);
  return areas.map((a, i) => ({
    // Negative ids: fixtures must never collide with OSM / server ids.
    id: -(i + 1),
    kind: 'swim_area',
    name: `Rhine swim area ${i + 1}`,
    source: 'ods-100270',
    source_id: null,
    lon: a.lon,
    lat: a.lat,
    geometry: a.geometry,
    properties: {},
  }));
}

// ---- cool places (server seed) ----------------------------------------------

async function makeCoolPlaces() {
  const fc = await readJson(join(repoRoot, 'server', 'seed', 'cool-places.geojson'));
  return fc.features.map((f, i) => {
    const [lon, lat] = f.geometry.coordinates;
    const props = {};
    for (const key of ['address', 'note', 'url']) {
      if (typeof f.properties[key] === 'string') props[key] = f.properties[key];
    }
    return {
      id: -(1000 + i + 1),
      kind: 'cool_place',
      name: f.properties.name,
      source: 'seed-cool-places',
      source_id: f.properties.name,
      lon,
      lat,
      geometry: { type: 'Point', coordinates: [lon, lat] },
      properties: props,
    };
  });
}

// ---- air temperature stations (data.bs.ch 100009 / 100082) ------------------

async function makeStations() {
  // Newest-first walk over the measurement dataset (see server/src/poller.rs):
  // the first row we see per station is its latest reading.
  const latest = new Map();
  for (let page = 0; page < AIR_PAGE_CAP; page++) {
    const url =
      `${ODS}/catalog/datasets/100009/records?limit=${AIR_PAGE_SIZE}` +
      `&offset=${page * AIR_PAGE_SIZE}&order_by=${encodeURIComponent('dates_max_date desc')}`;
    const { results } = await fetchJson(url);
    let discovered = 0;
    for (const r of results) {
      if (latest.has(r.name_original)) continue;
      latest.set(r.name_original, r);
      discovered++;
    }
    if (results.length < AIR_PAGE_SIZE || discovered === 0) break;
  }

  const rows = [...latest.values()].filter(
    (r) => typeof r.meta_airtemp === 'number' && r.coords && inBasel(r.coords.lon, r.coords.lat),
  );
  const newest = Math.max(...rows.map((r) => Date.parse(r.dates_max_date)));
  return rows
    .filter((r) => newest - Date.parse(r.dates_max_date) <= MAX_READING_AGE_MS)
    .sort((a, b) => a.name_original.localeCompare(b.name_original))
    .map((r) => ({
      id: r.name_original,
      kind: 'air',
      name: r.name_custom ?? r.name_original,
      source: 'data.bs.ch',
      lon: r.coords.lon,
      lat: r.coords.lat,
      geometry: { type: 'Point', coordinates: [r.coords.lon, r.coords.lat] },
      temperature_c: r.meta_airtemp,
      measured_at: r.dates_max_date,
    }));
}

// ---- main -------------------------------------------------------------------

const fountains = await makeFountains();
const swimAreas = await makeSwimAreas();
const coolPlaces = await makeCoolPlaces();
const stations = await makeStations();

const meta = {
  generated_at: new Date().toISOString(),
  counts: {
    fountains: fountains.length,
    swim_areas: swimAreas.length,
    cool_places: coolPlaces.length,
    stations: stations.length,
  },
  sources: {
    fountains: {
      origin: FOUNTAINS_URL,
      attribution: '© OpenStreetMap contributors',
      licence: 'ODbL 1.0 (https://www.openstreetmap.org/copyright)',
    },
    swim_areas: {
      origin: `${ODS}/catalog/datasets/100270`,
      attribution: 'Daten: Kanton Basel-Stadt (data.bs.ch)',
      licence: 'CC BY 4.0',
    },
    cool_places: {
      origin: 'server/seed/cool-places.geojson',
      note: 'Curated list from bs.ch heat prevention; addresses geocoded once with Nominatim (© OpenStreetMap contributors).',
    },
    stations: {
      origin: `${ODS}/catalog/datasets/100009 + 100082`,
      attribution: 'Daten: Kanton Basel-Stadt (data.bs.ch)',
      licence: 'CC BY 4.0',
    },
  },
};

await mkdir(outDir, { recursive: true });
const write = (name, value) => writeFile(join(outDir, name), JSON.stringify(value, null, 1) + '\n');
await Promise.all([
  write('fountains.json', fountains),
  write('swim_areas.json', swimAreas),
  write('cool_places.json', coolPlaces),
  write('stations.json', stations),
  write('meta.json', meta),
]);
console.log(meta.counts);
