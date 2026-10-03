<script lang="ts">
  import { onMount } from 'svelte';
  import { Map, AttributionControl, Marker } from 'maplibre-gl';
  import 'maplibre-gl/dist/maplibre-gl.css';
  import './worker';
  import { BASEL, INITIAL_ZOOM, filters, location, mapActions, view } from '../state/app.svelte';
  import { LocationError, getPosition } from '../location';
  import { showToast } from '../hud/toasts.svelte';
  import { addAppLayers, setFilterVisible } from './layers';
  import { useOverlays } from './overlays.svelte';
  import { loadStyle, type Basemap } from './basemap';
  import { addLocationLayer, updateLocationLayer } from './locationLayer';
  import { addRouteLayer, fitRoute, updateRouteLayer } from './routeLayer';
  import { addSelectionLayer, updateSelectionLayer } from './selectionLayer';
  import { bindInteraction } from './interaction';
  import { selection } from '../state/selection.svelte';
  import { routing, type Route } from '../routing';

  // Lets the parent (and later your layer modules) get the map instance
  // once it exists. Optional for now, but it is the hand-off point.
  let { onready }: { onready?: (map: Map) => void } = $props();

  let container: HTMLDivElement;

  // Set once the first style has loaded. Effects below only run after that.
  let map = $state<Map | null>(null);

  // The basemap currently loaded into the map, to avoid redundant setStyle calls.
  let loadedBasemap: Basemap | null = null;
  // Incremented per load so a slow fetch cannot overwrite a newer choice.
  let loadToken = 0;
  let disposed = false;

  onMount(() => {
    // No `style` here: we fetch and rewrite the swisstopo styles ourselves
    // (see basemap.ts) and hand the finished document to setStyle.
    const instance = new Map({
      container,
      center: BASEL,
      zoom: INITIAL_ZOOM,
      attributionControl: false,
    });

    loadBasemap(instance, view.basemap);

    instance.addControl(
      new AttributionControl({
        customAttribution: '© swisstopo',
        compact: true,
      }),
      'bottom-right',
    );

    // Fires for the initial style and again after every setStyle.
    // Everything we add on top of the basemap must be (re)created here.
    instance.on('style.load', () => {
      addAppLayers(instance);
      addLocationLayer(instance);
      // Route below the selection ring, both above our overlays.
      addRouteLayer(instance);
      addSelectionLayer(instance);
      applyFilters(instance);
    });

    // Tap to select, hover cursor (and the debug position pick).
    bindInteraction(instance);

    // Dev only: lets tests project feature coordinates to screen pixels.
    if (import.meta.env.DEV) (window as unknown as { __lanaMap: Map }).__lanaMap = instance;

    instance.once('load', () => {
      mapActions.locate = () => locate(instance);

      map = instance;
      onready?.(instance);
    });

    // Cleanup on unmount and on hot reload. Without this you leak
    // WebGL contexts and the map goes black after a few saves.
    return () => {
      disposed = true;
      instance.remove();
    };
  });

  // Switch the basemap when the HUD changes it. The style.load handler
  // above restores our own layers afterwards.
  $effect(() => {
    const basemap = view.basemap;
    if (!map) return;
    loadBasemap(map, basemap);
  });

  async function loadBasemap(m: Map, basemap: Basemap) {
    if (basemap === loadedBasemap) return;
    loadedBasemap = basemap;
    const token = ++loadToken;
    try {
      const style = await loadStyle(basemap);
      if (disposed || token !== loadToken) return;
      // `diff: true` lets MapLibre apply only what differs between styles.
      m.setStyle(style, { diff: true });
    } catch (err) {
      console.error('basemap load failed', err);
      if (token === loadToken) loadedBasemap = null;
    }
  }

  // Apply layer visibility whenever a filter changes.
  $effect(() => {
    if (!map) return;
    applyFilters(map);
  });

  // Crosshair cursor while picking a location (debug).
  $effect(() => {
    if (!map) return;
    map.getCanvas().style.cursor = location.picking ? 'crosshair' : '';
  });

  // Ring around the selected feature.
  $effect(() => {
    const coords = selection.current?.coords ?? null;
    if (!map) return;
    updateSelectionLayer(map, coords);
  });

  // The route line, framed once when a new route arrives.
  let framedRoute: Route | null = null;
  $effect(() => {
    const current = routing.current;
    if (!map) return;
    updateRouteLayer(map, current);
    if (current && current !== framedRoute) fitRoute(map, current);
    framedRoute = current;
  });

  // Accuracy circle follows the position (GPS fixes only).
  $effect(() => {
    const { coords, accuracyM } = location;
    if (!map) return;
    updateLocationLayer(map, coords, accuracyM);
  });

  // Marker for the user's position. A DOM marker (not a layer) survives
  // basemap switches on its own and is cheap for a single point.
  let positionMarker: Marker | null = null;
  $effect(() => {
    const coords = location.coords;
    if (!map) return;
    if (!coords) {
      positionMarker?.remove();
      positionMarker = null;
      return;
    }
    if (!positionMarker) {
      const el = document.createElement('div');
      el.className = 'position-marker';
      el.setAttribute('aria-label', 'Your location');
      positionMarker = new Marker({ element: el }).setLngLat(coords).addTo(map);
    } else {
      positionMarker.setLngLat(coords);
    }
  });

  // One GPS fix, then fly there. Never zooms out below the current zoom.
  async function locate(m: Map) {
    if (location.status === 'locating') return;
    location.status = 'locating';
    try {
      const fix = await getPosition();
      location.coords = fix.coords;
      location.accuracyM = fix.accuracyM;
      location.source = 'gps';
      location.status = 'idle';
      m.flyTo({ center: fix.coords, zoom: Math.max(m.getZoom(), 16) });
    } catch (err) {
      location.status = 'error';
      console.warn('locate failed', err);
      if (location.source === 'manual' && location.coords) {
        m.flyTo({ center: location.coords, zoom: Math.max(m.getZoom(), 16) });
        showToast('GPS nicht verfügbar – manuelle Position');
        return;
      }
      const denied = err instanceof LocationError && err.kind === 'denied';
      showToast(denied ? 'Standortzugriff verweigert' : 'Standort nicht verfügbar', {
        action: { label: 'Auf Karte wählen', run: () => (location.picking = true) },
      });
    }
  }

  function applyFilters(m: Map) {
    setFilterVisible(m, 'heat', filters.heat);
    setFilterVisible(m, 'water', filters.water);
  }

  // Keep the overlay sources in sync with the data store.
  useOverlays(() => map);
</script>

<div class="map" bind:this={container}></div>

<style>
  .map {
    position: absolute;
    inset: 0;
  }

  /* Created imperatively above, so it needs :global. */
  .map :global(.position-marker) {
    width: 18px;
    height: 18px;
    border-radius: 50%;
    background: #1f6feb;
    border: 3px solid white;
    box-shadow: 0 0 0 4px rgba(31, 111, 235, 0.3), 0 1px 4px rgba(0, 0, 0, 0.3);
  }
</style>
