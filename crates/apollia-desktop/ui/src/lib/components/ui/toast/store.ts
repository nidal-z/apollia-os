import { derived, writable, get } from "svelte/store";

export type ToastVariant = "success" | "error" | "info" | "loading" | "urgent";

export interface ToastItem {
  id: string;
  message: string;
  variant: ToastVariant;
  autoDismiss: number;
  showProgress: boolean;
  actionLabel?: string;
  onaction?: () => void;
  "data-testid"?: string;
}

export const MAX_VISIBLE_TOASTS = 4;

const DEFAULT_DURATION_MS = 4000;

export const toasts = writable<ToastItem[]>([]);

export const visibleToasts = derived(toasts, ($t) => $t.slice(0, MAX_VISIBLE_TOASTS));
export const queuedCount = derived(toasts, ($t) => Math.max(0, $t.length - MAX_VISIBLE_TOASTS));

let counter = 0;
const timers = new Map<string, ReturnType<typeof setTimeout>>();

export interface AddToastOptions {
  duration?: number;
  showProgress?: boolean;
  actionLabel?: string;
  onaction?: () => void;
  "data-testid"?: string;
}

function scheduleDismiss(id: string, duration: number) {
  if (duration <= 0) return;
  const timer = setTimeout(() => removeToast(id), duration);
  timers.set(id, timer);
}

function clearTimer(id: string) {
  const timer = timers.get(id);
  if (timer) {
    clearTimeout(timer);
    timers.delete(id);
  }
}

export function addToast(
  message: string,
  variant: ToastVariant = "info",
  options?: AddToastOptions,
): string {
  const id = `toast-${++counter}`;
  const defaultDuration = variant === "loading" ? 0 : DEFAULT_DURATION_MS;
  const duration = options?.duration ?? defaultDuration;
  const showProgress = options?.showProgress ?? (duration > 0);

  const item: ToastItem = {
    id,
    message,
    variant,
    autoDismiss: duration,
    showProgress: showProgress && duration > 0,
    actionLabel: options?.actionLabel,
    onaction: options?.onaction,
    "data-testid": options?.["data-testid"],
  };

  toasts.update((t) => [...t, item]);

  const list = get(toasts);
  const idx = list.findIndex((x) => x.id === id);
  if (idx < MAX_VISIBLE_TOASTS) scheduleDismiss(id, duration);

  return id;
}

export function updateToast(
  id: string,
  partial: Partial<Omit<ToastItem, "id">>,
) {
  let shouldReschedule = false;
  let newDuration = 0;

  toasts.update((t) =>
    t.map((item) => {
      if (item.id !== id) return item;
      const merged: ToastItem = { ...item, ...partial };
      if (partial.autoDismiss !== undefined || partial.variant !== undefined) {
        shouldReschedule = true;
        newDuration = merged.autoDismiss;
      }
      return merged;
    }),
  );

  if (shouldReschedule) {
    clearTimer(id);
    const list = get(toasts);
    const idx = list.findIndex((x) => x.id === id);
    if (idx !== -1 && idx < MAX_VISIBLE_TOASTS) scheduleDismiss(id, newDuration);
  }
}

export function pauseToast(id: string) {
  clearTimer(id);
}

export function resumeToast(id: string, remainingMs: number) {
  if (remainingMs > 0) scheduleDismiss(id, remainingMs);
}

export function removeToast(id: string) {
  clearTimer(id);
  let promotedId: string | null = null;
  let promotedDuration = 0;

  toasts.update((t) => {
    const filtered = t.filter((item) => item.id !== id);
    const promoted = filtered[MAX_VISIBLE_TOASTS - 1];
    if (promoted && !timers.has(promoted.id) && promoted.autoDismiss > 0) {
      promotedId = promoted.id;
      promotedDuration = promoted.autoDismiss;
    }
    return filtered;
  });

  if (promotedId) scheduleDismiss(promotedId, promotedDuration);
}

export function clearToasts() {
  for (const id of timers.keys()) clearTimer(id);
  toasts.set([]);
}
