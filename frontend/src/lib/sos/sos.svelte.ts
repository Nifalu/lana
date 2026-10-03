/**
 * SOS hook. The HUD button only calls `triggerSos()`; what an SOS actually
 * does (backend call, notifying contacts, ...) is plugged in from outside:
 *
 *   import { registerSosHandler, sos } from '../sos';
 *
 *   const unregister = registerSosHandler(async ({ coords, source }) => {
 *     await sendHelpRequest(coords);   // throw to show "SOS fehlgeschlagen"
 *     sos.active = true;               // optional: drives the button's look
 *   });
 *
 * Only one handler is active; registering another replaces it. Call the
 * returned function to unregister (it only removes its own handler).
 */
import { location, type LngLat } from '../state/app.svelte';
import { showToast } from '../hud/toasts.svelte';

export type SosContext = { coords: LngLat | null; source: 'gps' | 'manual' | null };
export type SosHandler = (ctx: SosContext) => void | Promise<void>;

/** The handler owner may set `active` to drive the button's active look. */
export const sos = $state({ active: false });

let handler: SosHandler | null = null;

export function registerSosHandler(fn: SosHandler): () => void {
  handler = fn;
  return () => {
    if (handler === fn) handler = null;
  };
}

/** Called by the SOS button. */
export async function triggerSos(): Promise<void> {
  const fn = handler;
  if (!fn) {
    console.info('SOS: no handler registered');
    showToast('SOS ist noch nicht verbunden');
    return;
  }
  const ctx: SosContext = {
    coords: location.coords ? [...location.coords] : null,
    source: location.source,
  };
  try {
    await fn(ctx);
  } catch (err) {
    console.error('SOS failed', err);
    showToast('SOS fehlgeschlagen');
  }
}
