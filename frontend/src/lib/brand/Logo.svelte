<script lang="ts">
  /**
   * Lana logo as a single animated SVG outline.
   *
   *  - static:   complete outline, no animation
   *  - loop:     chasing dash with a breathing gap (loading indicator)
   *  - original: chasing dash with a fixed gap
   *  - once:     draws the outline, holds, then fades out (intro / splash)
   *
   * The stroke uses `currentColor`, so set `color` on the parent to tint it.
   * The SVG fills its container; size it from the outside.
   */
  export type LogoVariant = 'original' | 'loop' | 'once' | 'static';

  let { variant = 'static' }: { variant?: LogoVariant } = $props();
</script>

<svg class="logo" viewBox="0 0 240 320" role="img" aria-label="Lana logo">
  <path
    class:outline-loop={variant === 'loop'}
    class:outline-original={variant === 'original'}
    class:outline-once={variant === 'once'}
    d="M107 30H133Q140 31 141 38L177 269Q178 287 162 292Q120 303 78 292Q62 287 63 269L99 38Q100 31 107 30Z"
    pathLength="100"
    fill="none"
    stroke="currentColor"
    stroke-width="2.5"
    stroke-linecap="round"
    stroke-linejoin="round"
  />
</svg>

<style>
  .logo {
    display: block;
    width: 100%;
    height: auto;
    overflow: visible;
  }

  .outline-original {
    stroke-dasharray: 90 10;
    animation: outline-chase 2.4s linear infinite;
  }

  .outline-loop {
    stroke-dasharray: 90 10;
    animation:
      outline-chase 2.4s linear infinite,
      outline-breathe 4.8s ease-in-out infinite;
  }

  .outline-once {
    stroke-dasharray: 100 100;
    animation: outline-reveal 3.6s ease-in-out both;
  }

  @keyframes outline-breathe {
    0%,
    100% {
      stroke-dasharray: 90 10;
    }
    25% {
      stroke-dasharray: 98 2;
    }
    75% {
      stroke-dasharray: 65 35;
    }
  }

  @keyframes outline-reveal {
    0% {
      stroke-dashoffset: 100;
      opacity: 1;
    }
    65%,
    80% {
      stroke-dashoffset: 0;
      opacity: 1;
    }
    100% {
      stroke-dashoffset: 0;
      opacity: 0;
    }
  }

  @keyframes outline-chase {
    from {
      stroke-dashoffset: 0;
    }
    to {
      stroke-dashoffset: -100;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .outline-original,
    .outline-loop,
    .outline-once {
      animation: none;
      stroke-dasharray: none;
    }
  }
</style>
