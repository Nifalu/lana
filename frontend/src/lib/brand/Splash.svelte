<script module lang="ts">
  /** How long the logo takes to draw itself; App keeps the splash up at least this long. */
  export const SPLASH_DRAW_MS = 1200;
</script>

<script lang="ts">
  /**
   * Full-screen opening overlay. The logo draws itself once (1.2 s); if the
   * app is still loading after that it keeps going with the loop. Fades out
   * when `visible` turns false (the app keeps it up for at least the draw).
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
  import { showToast } from '../hud/toasts.svelte';
  import soundUrl from '../../assets/lana-sound.mp3?inline';

  let { visible = true }: { visible?: boolean } = $props();

  let drawn = $state(false);

  // Debug builds say why the chime failed (the phone has no console at hand).
  const DEBUG = import.meta.env.VITE_RELEASE !== 'true';

  /** Plain audio element first; if the platform refuses it, Web Audio. */
  async function playChime(): Promise<void> {
    try {
      await new Audio(soundUrl).play();
      return;
    } catch (err) {
      console.warn('opening sound: audio element refused', err);
      try {
        const Ctx = window.AudioContext ?? (window as unknown as { webkitAudioContext: typeof AudioContext }).webkitAudioContext;
        const ctx = new Ctx();
        if (ctx.state === 'suspended') await ctx.resume();
        const bytes = await (await fetch(soundUrl)).arrayBuffer();
        const buffer = await ctx.decodeAudioData(bytes);
        const source = ctx.createBufferSource();
        source.buffer = buffer;
        source.connect(ctx.destination);
        source.start();
        if (ctx.state !== 'running') throw new Error(`audio context ${ctx.state}`);
      } catch (err2) {
        console.warn('opening sound: Web Audio failed too', err2);
        const name = (e: unknown) => (e instanceof Error ? `${e.name}: ${e.message}` : String(e));
        if (DEBUG) showToast(`Startton blockiert – ${name(err)} / ${name(err2)}`, { durationMs: 10000 });
      }
    }
  }

  onMount(() => {
    void playChime();
    const timer = setTimeout(() => (drawn = true), SPLASH_DRAW_MS);
    return () => clearTimeout(timer);
  });
</script>

{#if visible}
  <div class="splash" out:fade={{ duration: 400 }}>
    <div class="mark">
      <Logo variant={drawn ? 'loop' : 'draw'} />
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
