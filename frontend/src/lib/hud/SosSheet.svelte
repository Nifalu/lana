<script lang="ts">
  import { tick } from 'svelte';
  import { fly } from 'svelte/transition';
  import { location } from '../state/app.svelte';
  import { routing } from '../routing';
  import {
    NOTE_MAX,
    acceptIncoming,
    cancelConfirm,
    cancelOwn,
    formatDistance,
    ignoreIncoming,
    incomingView,
    resolveOwn,
    routeToPerson,
    sendSos,
    sosFlow,
    sosSheetKind,
  } from '../sos';

  /** Rendered height in px, so the HUD can lift the toast above the sheet. */
  let { height = $bindable(0) }: { height?: number } = $props();

  const kind = $derived(sosSheetKind());
  const incoming = $derived(incomingView());
  const own = $derived(sosFlow.own);

  let heading = $state<HTMLElement>();
  let note = $state('');

  // Move focus to the heading whenever the sheet (or its content) changes.
  $effect(() => {
    const key = kind === 'incoming' ? `incoming:${incoming?.request.id}` : `${kind}:${own?.request.status}`;
    if (!kind) {
      height = 0;
      return;
    }
    void key;
    void tick().then(() => heading?.focus({ preventScroll: true }));
  });

  // A fresh confirm sheet starts with an empty note.
  $effect(() => {
    if (kind !== 'confirm') note = '';
  });

  function onkeydown(e: KeyboardEvent) {
    if (e.key === 'Escape' && kind === 'confirm' && !e.defaultPrevented) cancelConfirm();
  }

  function flyParams() {
    const reduced = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
    const desktop = window.matchMedia('(min-width: 768px)').matches;
    return { duration: reduced ? 0 : 200, ...(desktop ? { x: -24 } : { y: 48 }) };
  }

  const hasRoute = $derived(routing.current !== null);
</script>

<svelte:window {onkeydown} />

{#if kind}
  <section
    class="sheet"
    class:alert={kind === 'incoming' || (kind === 'own' && own?.role === 'requester')}
    aria-label="SOS"
    bind:clientHeight={height}
    transition:fly={flyParams()}
  >
    {#if kind === 'confirm'}
      <h2 bind:this={heading} tabindex="-1">Hilfe anfordern?</h2>
      <p class="detail">
        Helfer in der Nähe (ca. 500 m) sehen deinen Standort{location.source === 'manual'
          ? ' (auf der Karte gewählt)'
          : ''}.
      </p>
      <label class="note">
        <span class="muted">Kurze Notiz (optional)</span>
        <textarea
          bind:value={note}
          maxlength={NOTE_MAX}
          rows="2"
          placeholder="z. B. schwindlig, brauche Wasser"
        ></textarea>
        <span class="muted counter">{note.length}/{NOTE_MAX}</span>
      </label>
      <div class="actions">
        <button class="secondary" type="button" onclick={cancelConfirm} disabled={sosFlow.sending}>
          Abbrechen
        </button>
        <button
          class="danger"
          type="button"
          disabled={sosFlow.sending}
          aria-busy={sosFlow.sending}
          onclick={() => sendSos(note)}
        >
          {sosFlow.sending ? 'Wird gesendet…' : 'Senden'}
        </button>
      </div>
    {:else if kind === 'own' && own}
      {#if own.role === 'requester'}
        {#if own.request.status === 'responded'}
          <h2 bind:this={heading} tabindex="-1">Hilfe ist unterwegs</h2>
          <p class="detail">Eine Person aus der Nähe kommt zu dir. Bleib möglichst, wo du bist.</p>
          <div class="actions">
            <button class="primary" type="button" disabled={sosFlow.busy} onclick={resolveOwn}>
              Erledigt
            </button>
          </div>
        {:else}
          <h2 bind:this={heading} tabindex="-1">Hilfe angefordert – warte auf Helfer…</h2>
          {#if own.request.note}<p class="detail muted">«{own.request.note}»</p>{/if}
          <div class="actions">
            <button class="secondary" type="button" disabled={sosFlow.busy} onclick={cancelOwn}>
              Abbrechen
            </button>
          </div>
        {/if}
      {:else}
        <h2 bind:this={heading} tabindex="-1">Du hilfst – Route zur Person</h2>
        {#if own.request.note}<p class="detail">«{own.request.note}»</p>{/if}
        {#if routing.current}
          <p class="detail">
            <strong>{formatDistance(routing.current.distanceKm * 1000)}</strong>
            {#if routing.current.source === 'stub'}<span class="muted">Luftlinie</span>{/if}
          </p>
        {/if}
        <div class="actions">
          {#if !hasRoute}
            <button
              class="secondary"
              type="button"
              disabled={routing.status === 'loading'}
              onclick={routeToPerson}
            >
              Route anzeigen
            </button>
          {/if}
          <button class="primary" type="button" disabled={sosFlow.busy} onclick={resolveOwn}>
            Erledigt
          </button>
        </div>
      {/if}
    {:else if kind === 'incoming' && incoming}
      {@const r = incoming.request}
      <h2 bind:this={heading} tabindex="-1" aria-live="assertive">
        Jemand in der Nähe braucht Hilfe
      </h2>
      <p class="detail">
        {#if incoming.distanceM !== null}<strong>{formatDistance(incoming.distanceM)}</strong> entfernt{/if}
        {#if incoming.count > 1}<span class="muted"> · {incoming.count} Anfragen offen</span>{/if}
      </p>
      {#if r.note}<p class="detail">«{r.note}»</p>{/if}
      <div class="actions">
        <button class="secondary" type="button" disabled={sosFlow.busy} onclick={() => ignoreIncoming(r.id)}>
          Ignorieren
        </button>
        <button
          class="primary"
          type="button"
          disabled={sosFlow.busy}
          aria-busy={sosFlow.busy}
          onclick={() => acceptIncoming(r)}
        >
          Ich helfe
        </button>
      </div>
    {/if}
  </section>
{/if}

<style>
  /* Same box as SelectionSheet, so the HUD treats both alike. */
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
    font-family: var(--font-ui, system-ui, -apple-system, 'Segoe UI', Roboto, sans-serif);
    box-shadow: 0 -2px 16px rgba(0, 0, 0, 0.22);
    border-top: 4px solid #1f6feb;
    overflow-y: auto;
    overscroll-behavior: contain;

    left: calc(env(safe-area-inset-left) + 16px);
    right: calc(env(safe-area-inset-right) + 16px);
    bottom: calc(env(safe-area-inset-bottom) + 92px);
    max-height: calc(100dvh - env(safe-area-inset-bottom) - 92px - env(safe-area-inset-top) - 72px);
    border-radius: 18px 18px 14px 14px;
  }

  .sheet.alert {
    border-top-color: #d1342f;
  }

  @media (min-width: 768px) {
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

  .detail {
    font-size: 14px;
    overflow-wrap: anywhere;
  }

  .muted {
    color: #5c6470;
    font-size: 13px;
  }

  .note {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin-top: 2px;
  }

  textarea {
    box-sizing: border-box;
    width: 100%;
    resize: none;
    padding: 8px 10px;
    border: 1.5px solid #c4c9d0;
    border-radius: 10px;
    font: inherit;
    font-size: 15px;
    color: inherit;
    background: white;
  }

  textarea:focus-visible {
    outline: 2px solid #1f6feb;
    outline-offset: 1px;
  }

  .counter {
    align-self: flex-end;
    font-size: 11px;
  }

  .actions {
    display: flex;
    gap: 8px;
    margin-top: 6px;
  }

  .actions button {
    flex: 1;
  }

  .primary,
  .secondary,
  .danger {
    min-height: 44px;
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

  .danger {
    border: none;
    background: #d1342f;
    color: white;
  }

  .secondary {
    border: 1.5px solid #1f6feb;
    background: white;
    color: #1558d6;
  }

  button:disabled {
    opacity: 0.6;
    cursor: progress;
  }

  button:focus-visible {
    outline: 2px solid #1f6feb;
    outline-offset: 2px;
  }
</style>
