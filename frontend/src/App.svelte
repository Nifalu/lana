<script lang="ts">
  import MapView from './lib/map/MapView.svelte';
  import Hud from './lib/hud/Hud.svelte';
  import Splash, { SPLASH_DRAW_MS } from './lib/brand/Splash.svelte';
  import { onMount } from 'svelte';
  import { preload } from './lib/data/store.svelte';
  import { startSos } from './lib/sos';

  let mapReady = $state(false);
  // The splash stays until the map is ready AND the logo has drawn itself.
  let introDone = $state(false);

  // Read cached data and start syncing right away, in parallel with the map.
  onMount(() => {
    void preload();
    startSos();
    const timer = setTimeout(() => (introDone = true), SPLASH_DRAW_MS + 250);
    return () => clearTimeout(timer);
  });
</script>

<div class="shell">
  <MapView onready={() => (mapReady = true)} />
  <Hud />
  <Splash visible={!(mapReady && introDone)} />
</div>

<style>
  .shell {
    position: relative;
    height: 100dvh;
    width: 100%;
  }
</style>
