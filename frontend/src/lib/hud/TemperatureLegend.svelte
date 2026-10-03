<script lang="ts">
  import { data } from '../data/store.svelte';
  import { TEMP_STOPS, freshReadings } from '../map/temperature';

  const readings = $derived(freshReadings(data.stations));

  const newest = $derived.by(() => {
    const ids = new Set(readings.map((r) => r.id));
    const times = data.stations
      .filter((s) => ids.has(s.id) && s.measured_at)
      .map((s) => Date.parse(s.measured_at!));
    return times.length ? new Date(Math.max(...times)) : null;
  });

  const min = TEMP_STOPS[0][0];
  const max = TEMP_STOPS[TEMP_STOPS.length - 1][0];
  const gradient = `linear-gradient(to right, ${TEMP_STOPS.map(
    ([t, [r, g, b]]) => `rgb(${r}, ${g}, ${b}) ${((t - min) / (max - min)) * 100}%`,
  ).join(', ')})`;
  const ticks = [16, 24, 32, 36];
</script>

<div class="legend" role="img" aria-label="Temperaturskala {min} bis {max} Grad, interpoliert aus {readings.length} Sensoren">
  <div class="bar" style:background={gradient}></div>
  <div class="ticks">
    {#each ticks as t (t)}
      <span style:left="{((t - min) / (max - min)) * 100}%">{t}°</span>
    {/each}
  </div>
  <p>
    {readings.length} Sensoren{newest
      ? ` · ${newest.toLocaleTimeString('de-CH', { hour: '2-digit', minute: '2-digit' })}`
      : ''}
  </p>
</div>

<style>
  .legend {
    width: 160px;
    padding: 8px 10px 6px;
    border-radius: 10px;
    background: rgba(255, 255, 255, 0.9);
    box-shadow: 0 1px 4px rgba(0, 0, 0, 0.2);
    color: #111214;
    font-family: var(--font-ui, system-ui, -apple-system, 'Segoe UI', sans-serif);
    font-size: 11px;
    font-weight: 500;
  }

  .bar {
    height: 8px;
    border-radius: 4px;
  }

  .ticks {
    position: relative;
    height: 14px;
    margin: 0 4px;
  }

  .ticks span {
    position: absolute;
    top: 2px;
    transform: translateX(-50%);
  }

  p {
    margin: 2px 0 0;
    color: #555;
  }
</style>
