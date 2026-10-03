<script lang="ts">
  import { onMount } from 'svelte';
  import { Map, AttributionControl, Marker, type MapMouseEvent } from 'maplibre-gl';
  import 'maplibre-gl/dist/maplibre-gl.css';
  import { BASEL, INITIAL_ZOOM, filters, location, mapActions, view } from '../state/app.svelte';
  import { LAYER, addAppLayers, setLayerVisible } from './layers';
  import { loadStyle, type Basemap } from './basemap';

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
      applyFilters(instance);
    });

    // Debug: while `location.picking`, the next tap sets the position.
    instance.on('click', (e: MapMouseEvent) => {
      if (!location.picking) return;
      location.coords = [e.lngLat.lng, e.lngLat.lat];
      location.picking = false;
    });

    instance.once('load', () => {
      mapActions.locate = () => {
        // TODO: replace with device GPS (browser geolocation / Tauri plugin).
        // Until then a manual pick (debug button) or Basel is "here".
        location.coords ??= BASEL;
        instance.flyTo({ center: location.coords, zoom: 15 });
      };

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

  function applyFilters(m: Map) {
    setLayerVisible(m, LAYER.heat, filters.heat);
    setLayerVisible(m, LAYER.water, filters.water);
  }
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
