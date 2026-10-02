<script lang="ts">
  import MapView from './lib/map/MapView.svelte';
  import Hud from './lib/hud/Hud.svelte';
</script>

<div class="shell">
  <MapView onready={(map) => console.log('map ready', map)} />
  <Hud />
</div>

<style>
  .shell {
    position: relative;
    height: 100dvh;
    width: 100%;
  }
</style>
