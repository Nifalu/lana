/**
 * Minimal toast messages for the HUD.
 *
 *   showToast('Standort nicht verfügbar', {
 *     action: { label: 'Auf Karte wählen', run: () => (location.picking = true) },
 *   });
 *
 * One toast at a time: a new one replaces the current one.
 */
export type ToastAction = { label: string; run: () => void };
export type Toast = { id: number; message: string; action?: ToastAction };

export const toast = $state({ current: null as Toast | null });

let nextId = 1;
let timer: ReturnType<typeof setTimeout> | undefined;

export function showToast(
  message: string,
  options: { action?: ToastAction; durationMs?: number } = {},
): void {
  clearTimeout(timer);
  const id = nextId++;
  toast.current = { id, message, action: options.action };
  timer = setTimeout(() => {
    if (toast.current?.id === id) toast.current = null;
  }, options.durationMs ?? 4000);
}

export function dismissToast(): void {
  clearTimeout(timer);
  toast.current = null;
}
