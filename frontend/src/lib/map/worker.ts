/**
 * Ship MapLibre's web worker with the build.
 *
 * MapLibre 6 locates its worker at runtime with a computed filename, which
 * the bundler cannot see, so production builds (and therefore the Tauri app)
 * would request a file that was never emitted and the map would never load.
 * `?worker&url` makes Vite bundle the worker together with its shared chunk
 * and gives us the final URL, which MapLibre then uses instead.
 *
 * Import this module once, before the first `new Map(...)`.
 */
import { setWorkerUrl } from 'maplibre-gl';
import workerUrl from 'maplibre-gl/dist/maplibre-gl-worker.mjs?worker&url';

setWorkerUrl(workerUrl);
