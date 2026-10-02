<script lang="ts">
  import MapView from './lib/map/MapView.svelte';
  import Hud from './lib/hud/Hud.svelte';
  import Splash from './lib/brand/Splash.svelte';

  let mapReady = $state(false);
</script>

<div class="shell">
  <MapView onready={() => (mapReady = true)} />
  <Hud />
  <Splash visible={!mapReady} />
</div>

<style>
  .shell {
    position: relative;
    height: 100dvh;
    width: 100%;
  }
</style>
