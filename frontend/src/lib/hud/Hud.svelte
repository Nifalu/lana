<script lang="ts">
  import HudButton from './HudButton.svelte';
</script>

<div class="hud">
  <div class="tools">
    <HudButton label="Toggle heatmap" onclick={() => console.log('heatmap')}>
      H
    </HudButton>
  </div>
</div>

<style>
  .hud {
    position: absolute;
    inset: 0;
    pointer-events: none;
    padding: env(safe-area-inset-top) env(safe-area-inset-right)
             env(safe-area-inset-bottom) env(safe-area-inset-left);
  }

  .tools {
    position: absolute;
    top: calc(env(safe-area-inset-top) + 12px);
    left: calc(env(safe-area-inset-left) + 12px);
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
</style>
