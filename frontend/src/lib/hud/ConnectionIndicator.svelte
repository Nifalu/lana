<script lang="ts">
  import { connection, syncNow } from '../data/store.svelte';

  function clock(iso: string | null): string | null {
    if (!iso) return null;
    const date = new Date(iso);
    if (Number.isNaN(date.getTime())) return null;
    return date.toLocaleTimeString('de-CH', { hour: '2-digit', minute: '2-digit' });
  }

  const info = $derived.by(() => {
    const time = clock(connection.lastSyncAt);
    switch (connection.status) {
      case 'live':
        return { tone: 'live', text: 'Live', full: 'Live: mit dem Server verbunden' };
      case 'offline':
        return {
          tone: 'offline',
          text: time ? `Offline · Stand ${time}` : 'Offline',
          full: time
            ? `Offline: zeigt gespeicherte Daten, Stand ${time} Uhr`
            : 'Offline: keine Verbindung zum Server',
        };
      case 'fixture':
        return {
          tone: 'fixture',
          text: 'Testdaten',
          full: 'Testdaten: kein Server verbunden, es werden eingebaute Beispieldaten gezeigt',
        };
      default:
        return { tone: 'connecting', text: 'Verbinde…', full: 'Verbindung zum Server wird aufgebaut' };
    }
  });
</script>

<div class="indicator" role="status" aria-label={info.full}>
  <button
    class="pill {info.tone}"
    class:syncing={connection.syncing}
    type="button"
    title="Tippen zum Aktualisieren"
    onclick={() => void syncNow()}
  >
    <span class="dot" aria-hidden="true"></span>
    <span class="text">{info.text}</span>
  </button>
</div>

<style>
  .indicator {
    position: absolute;
    top: calc(env(safe-area-inset-top) + 12px);
    left: calc(env(safe-area-inset-left) + 12px);
    /* Leaves room for the tool column on the right. */
    max-width: calc(100% - env(safe-area-inset-left) - env(safe-area-inset-right) - 88px);
  }

  .pill {
    pointer-events: auto;
    display: flex;
    align-items: center;
    gap: 8px;
    max-width: 100%;
    height: 32px;
    padding: 0 12px 0 10px;
    border: none;
    border-radius: 999px;
    background: rgba(17, 18, 20, 0.75);
    backdrop-filter: blur(8px);
    -webkit-backdrop-filter: blur(8px);
    color: #f1f1ef;
    font: inherit;
    font-size: 12px;
    font-weight: 600;
    white-space: nowrap;
    cursor: pointer;
    -webkit-tap-highlight-color: transparent;
  }

  .text {
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .dot {
    flex: none;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: #8b8f94;
  }

  .live .dot {
    background: #2ea043;
  }
  .offline .dot {
    background: #e3a008;
  }
  .fixture .dot {
    background: #9a6bf0;
  }

  .syncing .dot {
    animation: sync-pulse 1s ease-in-out infinite;
  }

  @keyframes sync-pulse {
    50% {
      opacity: 0.3;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .syncing .dot {
      animation: none;
    }
  }
</style>
