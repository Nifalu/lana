<script lang="ts">
  /**
   * Full-screen opening overlay. Shows the looping logo while `visible`
   * is true and fades out when it turns false.
   *
   * Plays the opening sound once on mount. The Tauri webview allows
   * media to start without a user gesture, so it plays there. Plain
   * browsers block it, and then it simply stays silent.
   *
   * The sound is inlined as a data URL (`?inline`): Tauri serves the bundle
   * over its own URL scheme without HTTP range support, which the macOS
   * media player needs to stream an audio file, so a separate .mp3 would
   * fail silently inside the app.
   */
  import { onMount } from 'svelte';
  import { fade } from 'svelte/transition';
  import Logo from './Logo.svelte';
  import soundUrl from '../../assets/lana-sound.mp3?inline';

  let { visible = true }: { visible?: boolean } = $props();

  onMount(() => {
    new Audio(soundUrl)
      .play()
      .catch((err) => console.warn('opening sound did not play:', err));
  });
</script>

{#if visible}
  <div class="splash" out:fade={{ duration: 400 }}>
    <div class="mark">
      <Logo variant="loop" />
    </div>
  </div>
{/if}

<style>
  .splash {
    position: absolute;
    inset: 0;
    z-index: 100;
    display: grid;
    place-items: center;
    background: #111214;
    color: #f1f1ef;
  }

  .mark {
    width: clamp(9rem, 30vw, 17rem);
  }
</style>
