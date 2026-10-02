<script lang="ts">
  import { onMount } from 'svelte';
  import { Map, NavigationControl, AttributionControl } from 'maplibre-gl';
  import 'maplibre-gl/dist/maplibre-gl.css';

  // Lets the parent (and later your layer modules) get the map instance
  // once it exists. Optional for now, but it is the hand-off point.
  let { onready }: { onready?: (map: Map) => void } = $props();

  let container: HTMLDivElement;

  onMount(() => {
    const map = new Map({
      container,
      style: 'https://vectortiles.geo.admin.ch/styles/ch.swisstopo.lightbasemap.vt/style.json',
      center: [8.2, 46.8],
      zoom: 7,
      attributionControl: false,
    });

    map.addControl(new NavigationControl(), 'top-right');
    map.addControl(
      new AttributionControl({
        customAttribution: '© swisstopo',
        compact: true,
      }),
    );

    map.on('load', () => onready?.(map));

    // Cleanup on unmount and on hot reload. Without this you leak
    // WebGL contexts and the map goes black after a few saves.
    return () => map.remove();
  });
</script>

<div class="map" bind:this={container}></div>

<style>
    .map {
      position: absolute;
      inset: 0;
    }
</style>
