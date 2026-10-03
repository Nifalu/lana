<script lang="ts">
  import { tick } from 'svelte';
  import { fly } from 'svelte/transition';
  import { selection } from '../state/selection.svelte';
  import {
    cooling,
    endRoute,
    isCoolCandidateShown,
    routing,
    showCoolCandidate,
    startRoute,
  } from '../routing';

  /** Rendered height in px, so the HUD can lift the toast above the sheet. */
  let { height = $bindable(0) }: { height?: number } = $props();

  // The selection, or while a route is shown without one, the route's target.
  const subject = $derived(selection.current ?? (routing.current ? routing.target : null));
  const onlyRoute = $derived(!selection.current && subject !== null);
  const route = $derived(routing.current);
  const loading = $derived(routing.status === 'loading');

  /** True when the route on the map leads to what the sheet describes. */
  const isRouteTarget = $derived(
    !!subject &&
      !!routing.current &&
      !!routing.target &&
      routing.target.coords[0] === subject.coords[0] &&
      routing.target.coords[1] === subject.coords[1],
  );

  // Browsing the KÜHL button's results: only while one of them is shown.
  const browsing = $derived(
    cooling.candidates.length > 1 && !!routing.current && isCoolCandidateShown(routing.target),
  );

  function step(delta: number) {
    if (!browsing) return;
    const next = cooling.index + delta;
    if (next >= 0 && next < cooling.candidates.length) showCoolCandidate(next);
  }

  // Horizontal swipe on the sheet: left = next place, right = previous.
  let swipeStart: { x: number; y: number } | null = null;
  function onpointerdown(e: PointerEvent) {
    swipeStart = browsing ? { x: e.clientX, y: e.clientY } : null;
  }
  function onpointerup(e: PointerEvent) {
    if (!swipeStart) return;
    const dx = e.clientX - swipeStart.x;
    const dy = e.clientY - swipeStart.y;
    swipeStart = null;
    if (Math.abs(dx) >= 50 && Math.abs(dx) > 1.5 * Math.abs(dy)) step(dx < 0 ? 1 : -1);
  }

  let sheet = $state<HTMLElement>();
  let heading = $state<HTMLElement>();

  // Focus: the heading on open, back where it was on close.
  let lastSubject: unknown = null;
  let opener: HTMLElement | null = null;
  $effect(() => {
    const current = subject;
    if (current && current !== lastSubject) {
      if (!lastSubject) opener = document.activeElement as HTMLElement | null;
      void tick().then(() => heading?.focus({ preventScroll: true }));
    }
    lastSubject = current;
  });

  function restoreFocus() {
    const active = document.activeElement;
    if (active && active !== document.body && !sheet?.contains(active)) return;
    const usable = opener && opener !== document.body && opener.isConnected;
    const target = usable ? opener : document.querySelector<HTMLElement>('.maplibregl-canvas');
    target?.focus({ preventScroll: true });
  }

  function close() {
    if (selection.current) selection.current = null;
    else endRoute();
    restoreFocus();
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key === 'Escape' && subject && !e.defaultPrevented) close();
    // Arrow keys page through the KÜHL results while the sheet has focus.
    if (browsing && sheet?.contains(document.activeElement)) {
      if (e.key === 'ArrowRight') step(1);
      if (e.key === 'ArrowLeft') step(-1);
    }
  }

  // Slides up on phones, in from the left on the docked desktop panel.
  function flyParams() {
    const reduced = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
    const desktop = window.matchMedia('(min-width: 768px)').matches;
    return { duration: reduced ? 0 : 200, ...(desktop ? { x: -24 } : { y: 48 }) };
  }

  // --- formatting ---------------------------------------------------------

  const text = (v: unknown): string | null => (typeof v === 'string' && v.trim() ? v.trim() : null);

  function distance(km: number): string {
    return km < 1 ? `${Math.round(km * 1000)} m` : `${km.toFixed(1)} km`;
  }

  function duration(seconds: number): string {
    const min = Math.max(1, Math.round(seconds / 60));
    return min < 60 ? `≈ ${min} min` : `≈ ${Math.floor(min / 60)} h ${min % 60} min`;
  }

  function measured(iso: unknown): string | null {
    const value = text(iso);
    const date = value ? new Date(value) : null;
    if (!date || Number.isNaN(date.getTime())) return null;
    const time = date.toLocaleTimeString('de-CH', { hour: '2-digit', minute: '2-digit' });
    const today = date.toDateString() === new Date().toDateString();
    return today ? `${time} Uhr` : `${date.toLocaleDateString('de-CH')}, ${time} Uhr`;
  }

  const coordinates = (c: [number, number]) => `${c[1].toFixed(5)}, ${c[0].toFixed(5)}`;

  /** Only plain web links are made clickable. */
  const safeUrl = (v: unknown): string | null => {
    const url = text(v);
    return url && /^https?:\/\//i.test(url) ? url : null;
  };
</script>

<svelte:window {onkeydown} />

{#if subject}
  {@const s = subject}
  {@const temperature = typeof s.properties.temperature_c === 'number' ? s.properties.temperature_c : null}
  {@const when = measured(s.properties.measured_at)}
  {@const address = text(s.properties.address)}
  {@const note = text(s.properties.note)}
  {@const url = safeUrl(s.properties.url)}
  <section
    class="sheet"
    aria-label="Ausgewählter Ort"
    bind:this={sheet}
    bind:clientHeight={height}
    transition:fly={flyParams()}
    {onpointerdown}
    {onpointerup}
    onpointercancel={() => (swipeStart = null)}
  >
    <header>
      <div class="titles">
        <h2 bind:this={heading} tabindex="-1">
          {onlyRoute ? 'Route zu ' : ''}{s.name ?? s.label}
        </h2>
        {#if s.name}<p class="kind">{s.label}</p>{/if}
      </div>
      <button
        class="close"
        type="button"
        aria-label={onlyRoute ? 'Route beenden' : 'Schliessen'}
        onclick={close}
      >
        ×
      </button>
    </header>

    {#if temperature !== null}
      <p class="temperature">
        <strong>{temperature.toFixed(1)} °C</strong>
        {#if when}<span class="muted">gemessen {when}</span>{/if}
      </p>
    {/if}
    {#if address}<p class="detail">{address}</p>{/if}
    {#if note}<p class="detail muted">{note}</p>{/if}
    {#if url}
      <p class="detail">
        <a href={url} target="_blank" rel="noopener noreferrer">{url.replace(/^https?:\/\//i, '')}</a>
      </p>
    {/if}

    <p class="coords" aria-label="Koordinaten">{coordinates(s.coords)}</p>

    {#if route && isRouteTarget}
      {#if browsing}
        <nav class="pager" aria-label="Kühle Orte in der Nähe">
          <button
            type="button"
            aria-label="Vorheriger Ort"
            disabled={cooling.index === 0}
            onclick={() => step(-1)}
          >
            ‹
          </button>
          <span aria-live="polite">{cooling.index + 1} / {cooling.candidates.length}</span>
          <button
            type="button"
            aria-label="Nächster Ort"
            disabled={cooling.index === cooling.candidates.length - 1}
            onclick={() => step(1)}
          >
            ›
          </button>
        </nav>
      {/if}
      <div class="route" aria-live="polite">
        <p class="stats">
          <strong>{distance(route.distanceKm)}</strong>
          <span>{duration(route.durationS)} zu Fuss</span>
        </p>
        {#if route.source === 'stub'}
          <p class="hint">Luftlinie (Routing folgt)</p>
        {/if}
      </div>
      <button class="secondary" type="button" onclick={endRoute}>Route beenden</button>
    {:else}
      {#if route && routing.target && !onlyRoute}
        <p class="other-route">
          Aktive Route: {distance(route.distanceKm)} {duration(route.durationS)}
          <button class="link" type="button" onclick={endRoute}>Beenden</button>
        </p>
      {/if}
      {#if !onlyRoute}
        <button
          class="primary"
          type="button"
          disabled={loading}
          aria-busy={loading}
          onclick={() => startRoute(s)}
        >
          {loading ? 'Route wird berechnet…' : 'Route hierher'}
        </button>
      {/if}
    {/if}
  </section>
{/if}

<style>
  .pager {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin: 8px 0 4px;
    font-size: 13px;
    color: #555;
  }

  .pager button {
    width: 44px;
    height: 32px;
    border: 1px solid rgba(0, 0, 0, 0.12);
    border-radius: 8px;
    background: #fff;
    color: #111214;
    font-size: 20px;
    line-height: 1;
    cursor: pointer;
  }

  .pager button:disabled {
    opacity: 0.35;
    cursor: default;
  }

  .sheet {
    position: absolute;
    pointer-events: auto;
    box-sizing: border-box;
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 14px 16px 16px;
    background: rgba(255, 255, 255, 0.96);
    color: #111214;
    /* The app sets no font yet; a brand font can define --font-ui. */
    font-family: var(--font-ui, system-ui, -apple-system, 'Segoe UI', Roboto, sans-serif);
    box-shadow: 0 -2px 16px rgba(0, 0, 0, 0.22);
    overflow-y: auto;
    overscroll-behavior: contain;

    /* Phone: above the tool bar, full width minus gutters. */
    left: calc(env(safe-area-inset-left) + 16px);
    right: calc(env(safe-area-inset-right) + 16px);
    bottom: calc(env(safe-area-inset-bottom) + 92px);
    max-height: calc(100dvh - env(safe-area-inset-bottom) - 92px - env(safe-area-inset-top) - 72px);
    border-radius: 18px 18px 14px 14px;
  }

  @media (min-width: 768px) {
    /* Docked panel on the left, below the connection pill. */
    .sheet {
      top: calc(env(safe-area-inset-top) + 56px);
      bottom: auto;
      right: auto;
      left: calc(env(safe-area-inset-left) + 12px);
      width: 320px;
      max-height: calc(100dvh - env(safe-area-inset-top) - 56px - 100px);
      border-radius: 14px;
      box-shadow: 0 2px 16px rgba(0, 0, 0, 0.22);
    }
  }

  header {
    display: flex;
    align-items: flex-start;
    gap: 8px;
  }

  .titles {
    flex: 1;
    min-width: 0;
  }

  h2 {
    margin: 0;
    font-size: 18px;
    line-height: 1.25;
    overflow-wrap: anywhere;
    outline: none;
  }

  p {
    margin: 0;
  }

  .kind {
    margin-top: 2px;
    font-size: 13px;
    color: #4d5560;
  }

  .temperature {
    font-size: 15px;
    display: flex;
    flex-wrap: wrap;
    gap: 4px 10px;
    align-items: baseline;
  }

  .detail {
    font-size: 14px;
    overflow-wrap: anywhere;
  }

  .muted {
    color: #5c6470;
    font-size: 13px;
  }

  a {
    color: #1558d6;
  }

  .coords {
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 11px;
    color: #6b727c;
  }

  .close {
    flex: none;
    width: 36px;
    height: 36px;
    margin: -6px -8px 0 0;
    border: none;
    border-radius: 50%;
    background: transparent;
    color: #111214;
    font-size: 26px;
    line-height: 1;
    cursor: pointer;
    -webkit-tap-highlight-color: transparent;
  }

  .close:hover {
    background: rgba(0, 0, 0, 0.07);
  }

  .route {
    margin-top: 4px;
    padding: 10px 12px;
    border-radius: 10px;
    background: #eaf1fe;
  }

  .stats {
    display: flex;
    flex-wrap: wrap;
    gap: 2px 10px;
    align-items: baseline;
    font-size: 15px;
  }

  .stats strong {
    font-size: 20px;
  }

  .hint {
    margin-top: 2px;
    font-size: 12px;
    color: #5c6470;
  }

  .other-route {
    margin-top: 4px;
    font-size: 13px;
    color: #4d5560;
  }

  .link {
    border: none;
    background: none;
    padding: 4px 6px;
    color: #1558d6;
    font: inherit;
    font-weight: 600;
    cursor: pointer;
  }

  .primary,
  .secondary {
    min-height: 44px;
    margin-top: 6px;
    padding: 0 16px;
    border-radius: 12px;
    font: inherit;
    font-size: 15px;
    font-weight: 600;
    cursor: pointer;
    -webkit-tap-highlight-color: transparent;
  }

  .primary {
    border: none;
    background: #1f6feb;
    color: white;
  }

  .primary:disabled {
    opacity: 0.7;
    cursor: progress;
    animation: busy-pulse 1s ease-in-out infinite;
  }

  .secondary {
    border: 1.5px solid #1f6feb;
    background: white;
    color: #1558d6;
  }

  button:focus-visible,
  a:focus-visible {
    outline: 2px solid #1f6feb;
    outline-offset: 2px;
  }

  @keyframes busy-pulse {
    50% {
      opacity: 0.45;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .primary:disabled {
      animation: none;
    }
  }
</style>
