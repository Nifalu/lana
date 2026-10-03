/**
 * SOS: asking for help and helping, through the middleware (see server.ts).
 *
 *   requester: SOS button -> confirm -> POST help request -> wait for a
 *              helper -> "Erledigt" (resolve) or "Abbrechen" (cancel)
 *   helper:    switch "Ich kann helfen" on -> position is shared (every 5 min
 *              and when it changes) -> help_request_new -> "Ich helfe" ->
 *              route to the person -> "Erledigt"
 *
 * The live link is one EventSource, open from app start (requesters need the
 * status updates too). `startSos()` wires everything up once.
 */
import { getPosition } from '../location';
import { showToast } from '../hud/toasts.svelte';
import { endRoute, startRoute } from '../routing';
import { location, mapActions, type LngLat } from '../state/app.svelte';
import { selection } from '../state/selection.svelte';
import {
  ApiError,
  createHelpRequest,
  fromLonLat,
  helpRequestAction,
  listHelpRequests,
  openEvents,
  putDevice,
  type HelpRequest,
} from './server';

export type SosRole = 'requester' | 'helper';
export type SosOwn = { request: HelpRequest; role: SosRole };
export type SosSheetKind = 'confirm' | 'own' | 'incoming';

const HELPER_KEY = 'lana-helper';
const OWN_KEY = 'lana-sos-own';

/** True while this device has an own, unfinished help request (drives the SOS button). */
export const sos = $state({ active: false });

/** State of the link to the middleware's event stream. */
export const sosConnection = $state({
  status: 'connecting' as 'connecting' | 'open' | 'closed',
});

/** "Ich kann helfen": opt-in, off by default. */
export const helper = $state({ on: readStored(HELPER_KEY) === '1' });

export const sosFlow = $state({
  /** The confirm sheet is open. */
  confirming: false,
  /** A request is being sent. */
  sending: false,
  /** A respond / resolve / cancel call is running. */
  busy: false,
  /** The request this device is part of, as requester or helper. */
  own: null as SosOwn | null,
  /** Open requests from others, not yet taken or ignored. */
  incoming: [] as HelpRequest[],
});

export const NOTE_MAX = 200;

const REFRESH_MS = 5 * 60_000;
const DEBOUNCE_MS = 1500;
const RADIUS_M = 500;

// Requests the user ignored, took, or finished: never shown again.
const dismissed = new Set<string>();
// Request being claimed right now, so its own `responded` event is not
// mistaken for someone else getting there first.
let pendingRespond: string | null = null;

// --- storage ----------------------------------------------------------------

function readStored(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

function writeStored(key: string, value: string | null): void {
  try {
    if (value === null) localStorage.removeItem(key);
    else localStorage.setItem(key, value);
  } catch {
    // Storage blocked: the state just does not survive a reload.
  }
}

// --- derived views ----------------------------------------------------------

const toRad = (deg: number) => (deg * Math.PI) / 180;

/** Great-circle distance in metres. */
function distanceM(a: LngLat, b: LngLat): number {
  const dLat = toRad(b[1] - a[1]);
  const dLon = toRad(b[0] - a[0]);
  const h =
    Math.sin(dLat / 2) ** 2 + Math.cos(toRad(a[1])) * Math.cos(toRad(b[1])) * Math.sin(dLon / 2) ** 2;
  return 2 * 6_371_008.8 * Math.asin(Math.sqrt(h));
}

/** "≈ 240 m" / "≈ 1.2 km". */
export function formatDistance(m: number): string {
  if (m < 1000) return `≈ ${Math.max(10, Math.round(m / 10) * 10)} m`;
  return `≈ ${(m / 1000).toFixed(1)} km`;
}

export type IncomingView = { request: HelpRequest; distanceM: number | null; count: number };

/** The nearest incoming request (the first one when there is no position yet). */
export function incomingView(): IncomingView | null {
  const list = sosFlow.incoming;
  if (list.length === 0) return null;
  const here = location.coords;
  const scored = list.map((request) => ({
    request,
    distanceM: here ? distanceM(here, fromLonLat(request.location)) : null,
  }));
  if (here) scored.sort((a, b) => a.distanceM! - b.distanceM!);
  return { ...scored[0], count: list.length };
}

/** Which SOS sheet is showing, if any. It takes precedence over the selection sheet. */
export function sosSheetKind(): SosSheetKind | null {
  if (sosFlow.confirming) return 'confirm';
  if (sosFlow.own) return 'own';
  if (sosFlow.incoming.length > 0) return 'incoming';
  return null;
}

/** Where the map shows red markers: open requests on offer, and the person being helped. */
export function sosMarkers(): LngLat[] {
  const points = sosFlow.incoming.map((r) => fromLonLat(r.location));
  const own = sosFlow.own;
  if (own?.role === 'helper') points.push(fromLonLat(own.request.location));
  return points;
}

// --- own request ---------------------------------------------------------------

function setOwn(own: SosOwn | null): void {
  sosFlow.own = own;
  sos.active = own?.role === 'requester';
  writeStored(OWN_KEY, own ? JSON.stringify(own) : null);
}

function restoreOwn(): void {
  const raw = readStored(OWN_KEY);
  if (!raw) return;
  try {
    const own = JSON.parse(raw) as SosOwn;
    if (own?.request?.id && (own.role === 'requester' || own.role === 'helper')) setOwn(own);
  } catch {
    writeStored(OWN_KEY, null);
  }
}

/** The own request is over: back to the map. */
function finishOwn(message?: string): void {
  const own = sosFlow.own;
  if (!own) return;
  dismissed.add(own.request.id);
  setOwn(null);
  if (own.role === 'helper') endRoute();
  if (message) showToast(message);
}

function buzz(): void {
  // Browsers refuse (and log an error) before the first tap on the page.
  if (navigator.userActivation && !navigator.userActivation.hasBeenActive) return;
  try {
    navigator.vibrate?.([200, 100, 200]);
  } catch {
    // Not supported here.
  }
}

// --- incoming requests -----------------------------------------------------------

function addIncoming(request: HelpRequest, notify: boolean): void {
  if (!helper.on) return;
  if (dismissed.has(request.id) || sosFlow.own?.request.id === request.id) return;
  if (sosFlow.incoming.some((r) => r.id === request.id)) return;
  sosFlow.incoming.push(request);
  if (notify) buzz();
}

function removeIncoming(id: string): void {
  const index = sosFlow.incoming.findIndex((r) => r.id === id);
  if (index >= 0) sosFlow.incoming.splice(index, 1);
}

/** Only the request the user is looking at counts as "on screen". */
function visibleIncomingId(): string | null {
  if (sosSheetKind() !== 'incoming') return null;
  return incomingView()?.request.id ?? null;
}

function onNew(request: HelpRequest): void {
  if (request.status === 'open') addIncoming(request, true);
}

function onUpdated(request: HelpRequest): void {
  const own = sosFlow.own;
  if (own && own.request.id === request.id) {
    updateOwn(own, request);
    return;
  }
  if (request.status === 'open') {
    addIncoming(request, false);
    return;
  }
  if (request.status === 'responded' && pendingRespond === request.id) return;
  const wasVisible = visibleIncomingId() === request.id;
  const known = sosFlow.incoming.some((r) => r.id === request.id);
  removeIncoming(request.id);
  if (known && wasVisible) {
    showToast(request.status === 'responded' ? 'Bereits übernommen' : 'Anfrage nicht mehr aktuell');
  }
}

function updateOwn(own: SosOwn, request: HelpRequest): void {
  if (request.status === 'resolved') {
    finishOwn(own.role === 'requester' ? 'Erledigt – gute Besserung!' : 'Einsatz beendet – danke fürs Helfen!');
  } else if (request.status === 'cancelled') {
    finishOwn(own.role === 'requester' ? 'Hilfeanfrage abgebrochen' : 'Die Anfrage wurde zurückgezogen');
  } else {
    const becameResponded = own.request.status !== 'responded' && request.status === 'responded';
    setOwn({ ...own, request });
    if (becameResponded && own.role === 'requester') {
      buzz();
      showToast('Hilfe ist unterwegs');
    }
  }
}

// --- requester flow ----------------------------------------------------------------

/** Called by the SOS button. Locates if needed, then asks for confirmation. */
export async function triggerSos(): Promise<void> {
  if (sosFlow.own) {
    showToast(sosFlow.own.role === 'helper' ? 'Du hilfst gerade – zuerst erledigen' : 'Hilfe ist bereits angefordert');
    return;
  }
  if (sosFlow.confirming) return;
  if (!location.coords) await mapActions.locate();
  if (!location.coords) {
    showToast('Zuerst Standort bestimmen', {
      action: { label: 'Auf Karte wählen', run: () => (location.picking = true) },
    });
    return;
  }
  sosFlow.confirming = true;
}

export function cancelConfirm(): void {
  if (!sosFlow.sending) sosFlow.confirming = false;
}

/** The confirm sheet's "Senden". */
export async function sendSos(note: string): Promise<void> {
  const here = location.coords;
  if (!here || sosFlow.sending) return;
  sosFlow.sending = true;
  try {
    const request = await createHelpRequest(here, note.trim().slice(0, NOTE_MAX) || undefined);
    sosFlow.confirming = false;
    setOwn({ request, role: 'requester' });
  } catch (err) {
    console.error('creating the help request failed', err);
    showToast('Hilfe konnte nicht angefordert werden');
  } finally {
    sosFlow.sending = false;
  }
}

/** Requester: withdraw while still open. */
export async function cancelOwn(): Promise<void> {
  await finishVia('cancel');
}

/** Requester or helper: the matter is settled. */
export async function resolveOwn(): Promise<void> {
  await finishVia('resolve');
}

async function finishVia(action: 'cancel' | 'resolve'): Promise<void> {
  const own = sosFlow.own;
  if (!own || sosFlow.busy) return;
  sosFlow.busy = true;
  try {
    const request = await helpRequestAction(own.request.id, action);
    if (sosFlow.own?.request.id === own.request.id) updateOwn(own, request);
  } catch (err) {
    console.error(`${action} failed`, err);
    if (err instanceof ApiError && (err.status === 409 || err.status === 404)) {
      // State moved on without us (e.g. a helper arrived): take the server's word.
      showToast(action === 'cancel' ? 'Jemand hilft bereits' : 'Anfrage nicht mehr aktuell');
      await syncOwn();
    } else {
      showToast('Keine Verbindung – bitte erneut versuchen');
    }
  } finally {
    sosFlow.busy = false;
  }
}

/** Reload the own request's status from the middleware (after a reload or reconnect). */
async function syncOwn(): Promise<void> {
  const own = sosFlow.own;
  if (!own) return;
  try {
    // There is no get-by-id; the request is found next to its own location.
    const nearby = await listHelpRequests(fromLonLat(own.request.location), 50);
    const fresh = nearby.find((r) => r.id === own.request.id);
    if (!fresh) finishOwn();
    else if (sosFlow.own?.request.id === own.request.id) updateOwn(own, fresh);
  } catch (err) {
    console.warn('syncing the own request failed', err);
  }
}

// --- helper flow --------------------------------------------------------------------

/** Ignore the shown request. */
export function ignoreIncoming(id: string): void {
  dismissed.add(id);
  removeIncoming(id);
}

/** "Ich helfe": claim the request, then route to the person. */
export async function acceptIncoming(request: HelpRequest): Promise<void> {
  if (sosFlow.busy || sosFlow.own) return;
  sosFlow.busy = true;
  pendingRespond = request.id;
  try {
    const claimed = await helpRequestAction(request.id, 'respond');
    removeIncoming(request.id);
    setOwn({ request: claimed, role: 'helper' });
    void routeToPerson();
  } catch (err) {
    if (err instanceof ApiError && (err.status === 409 || err.status === 404)) {
      showToast('Bereits übernommen');
      dismissed.add(request.id);
      removeIncoming(request.id);
    } else {
      console.error('respond failed', err);
      showToast('Keine Verbindung – bitte erneut versuchen');
    }
  } finally {
    pendingRespond = null;
    sosFlow.busy = false;
  }
}

/** Route from here to the person being helped, shown like "Route hierher". */
export async function routeToPerson(): Promise<void> {
  const own = sosFlow.own;
  if (!own || own.role !== 'helper') return;
  // The sheet and the route belong to the SOS now; an old selection would
  // pop up again behind it.
  selection.current = null;
  await startRoute({
    coords: fromLonLat(own.request.location),
    name: 'Person in Not',
    kind: 'sos',
    label: 'Hilfeanfrage',
    source: 'lana',
    properties: {},
  });
}

// --- sharing the helper position ---------------------------------------------------

let lastSent: { coords: LngLat | null; at: number } | null = null;

const sameCoords = (a: LngLat | null, b: LngLat | null) =>
  a === b || (!!a && !!b && a[0] === b[0] && a[1] === b[1]);

/**
 * Quietly refresh a GPS position, without moving the map. A manually picked
 * position is the user's choice and stays.
 */
async function refreshPosition(): Promise<void> {
  if (location.source === 'manual' && location.coords) return;
  try {
    const fix = await getPosition();
    location.coords = fix.coords;
    location.accuracyM = fix.accuracyM;
    location.source = 'gps';
  } catch (err) {
    console.warn('position refresh failed', err);
  }
}

/** PUT the device state; false when the middleware could not be reached. */
async function pushDevice(): Promise<boolean> {
  if (!helper.on) return false;
  const coords = location.coords ? ([...location.coords] as LngLat) : null;
  try {
    await putDevice(true, coords);
    lastSent = { coords, at: Date.now() };
    return true;
  } catch (err) {
    console.warn('sharing the helper position failed', err);
    return false;
  }
}

/** Switch "Ich kann helfen" on or off. */
export async function setHelper(on: boolean): Promise<void> {
  if (helper.on === on) return;
  helper.on = on;
  writeStored(HELPER_KEY, on ? '1' : '0');
  if (!on) {
    lastSent = null;
    sosFlow.incoming.length = 0;
    try {
      await putDevice(false, null);
    } catch (err) {
      console.warn('switching helper mode off failed', err);
      showToast('Keine Verbindung – Helfer-Status konnte nicht gesendet werden');
    }
    return;
  }
  if (!location.coords) await mapActions.locate();
  if (!helper.on) return;
  if (!location.coords) {
    showToast('Zum Helfen wird dein Standort benötigt', {
      action: { label: 'Auf Karte wählen', run: () => (location.picking = true) },
    });
  }
  if (await pushDevice()) await catchUp();
  else showToast('Keine Verbindung – Helfer-Status konnte nicht gesendet werden');
}

export const toggleHelper = () => setHelper(!helper.on);

/** Fetch open requests nearby that arrived while the stream was down. */
async function catchUp(): Promise<void> {
  const here = location.coords;
  if (!helper.on || !here) return;
  try {
    const open = await listHelpRequests(here, RADIUS_M, 'open', true);
    if (!helper.on) return;
    const ids = new Set(open.map((r) => r.id));
    // Anything on offer that is gone from the list was taken or withdrawn
    // while we were not listening.
    for (const r of [...sosFlow.incoming]) if (!ids.has(r.id)) removeIncoming(r.id);
    for (const request of open) addIncoming(request, true);
  } catch (err) {
    console.warn('catching up on help requests failed', err);
  }
}

// --- live events ----------------------------------------------------------------------

let events: EventSource | null = null;
let reconnectTimer: ReturnType<typeof setTimeout> | undefined;

function parse(e: Event): HelpRequest | null {
  try {
    return JSON.parse((e as MessageEvent<string>).data) as HelpRequest;
  } catch {
    console.warn('unreadable help request event');
    return null;
  }
}

async function connect(): Promise<void> {
  clearTimeout(reconnectTimer);
  events?.close();
  sosConnection.status = 'connecting';
  let source: EventSource;
  try {
    source = await openEvents();
  } catch (err) {
    console.warn('opening the event stream failed', err);
    sosConnection.status = 'closed';
    reconnectTimer = setTimeout(() => void connect(), 10_000);
    return;
  }
  events = source;
  source.onopen = () => {
    sosConnection.status = 'open';
    void syncOwn();
    void catchUp();
  };
  source.onerror = () => {
    // CONNECTING: the browser retries by itself. CLOSED: it gave up (bad
    // response), so reopen the stream ourselves after a pause.
    if (source.readyState === EventSource.CLOSED) {
      sosConnection.status = 'closed';
      reconnectTimer = setTimeout(() => void connect(), 10_000);
    } else {
      sosConnection.status = 'connecting';
    }
  };
  source.addEventListener('help_request_new', (e) => {
    const request = parse(e);
    if (request) onNew(request);
  });
  source.addEventListener('help_request_updated', (e) => {
    const request = parse(e);
    if (request) onUpdated(request);
  });
}

// --- start ------------------------------------------------------------------------------

let started = false;
let stop: (() => void) | null = null;

/** Open the event stream and run the helper refresh. Call once at app start. */
export function startSos(): void {
  if (started) return;
  started = true;
  restoreOwn();
  void connect();

  // Helper position: first right away, then every 5 minutes.
  const refresh = async () => {
    if (!helper.on) return;
    await refreshPosition();
    if (await pushDevice()) await catchUp();
  };
  void refresh();
  const interval = setInterval(() => void refresh(), REFRESH_MS);

  // Coming back to the app: the stream may have dropped and the position aged.
  const onVisible = () => {
    if (document.visibilityState !== 'visible') return;
    if (sosConnection.status === 'closed') void connect();
    void syncOwn();
    if (helper.on && (!lastSent || Date.now() - lastSent.at > REFRESH_MS)) void refresh();
    else void catchUp();
  };
  document.addEventListener('visibilitychange', onVisible);

  // A new position (GPS button, manual pick): share it after things settle.
  let debounce: ReturnType<typeof setTimeout> | undefined;
  const cleanupEffect = $effect.root(() => {
    $effect(() => {
      const coords = location.coords;
      if (!helper.on || !coords) return;
      clearTimeout(debounce);
      debounce = setTimeout(async () => {
        if (!helper.on || sameCoords(lastSent?.coords ?? null, location.coords)) return;
        if (await pushDevice()) await catchUp();
      }, DEBOUNCE_MS);
    });
  });

  stop = () => {
    clearInterval(interval);
    clearTimeout(debounce);
    clearTimeout(reconnectTimer);
    document.removeEventListener('visibilitychange', onVisible);
    cleanupEffect();
    events?.close();
    events = null;
    started = false;
  };
}

if (import.meta.hot) import.meta.hot.dispose(() => stop?.());
