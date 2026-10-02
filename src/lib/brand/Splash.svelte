<script lang="ts">
  /**
   * Full-screen opening overlay. Shows the looping logo while `visible`
   * is true and fades out when it turns false.
   *
   * Plays the opening sound once on mount. The Tauri webview allows
   * media to start without a user gesture, so it plays there. Plain
   * browsers block it, and then it simply stays silent.
   */
  import { onMount } from 'svelte';
  import { fade } from 'svelte/transition';
  import Logo from './Logo.svelte';
  import soundUrl from '../../assets/lana-sound.mp3';

  let { visible = true }: { visible?: boolean } = $props();

  onMount(() => {
    new Audio(soundUrl)
      .play()
      .catch(() => console.debug('opening sound: autoplay blocked by host'));
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
