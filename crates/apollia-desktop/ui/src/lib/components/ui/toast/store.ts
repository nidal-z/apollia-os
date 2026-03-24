import { writable } from "svelte/store";

export interface ToastItem {
  id: string;
  message: string;
  variant: "success" | "error" | "info";
  actionLabel?: string;
  onaction?: () => void;
  "data-testid"?: string;
}

export const toasts = writable<ToastItem[]>([]);

let counter = 0;

const DEFAULT_DURATION_MS = 4000;

export interface AddToastOptions {
  duration?: number;
  actionLabel?: string;
  onaction?: () => void;
  "data-testid"?: string;
}

export function addToast(
  message: string,
  variant: ToastItem["variant"] = "info",
  options?: AddToastOptions,
) {
  const id = `toast-${++counter}`;
  const duration = options?.duration ?? DEFAULT_DURATION_MS;

  toasts.update((t) => [
    ...t,
    {
      id,
      message,
      variant,
      actionLabel: options?.actionLabel,
      onaction: options?.onaction,
      "data-testid": options?.["data-testid"],
    },
  ]);

  setTimeout(() => removeToast(id), duration);
}

export function removeToast(id: string) {
  toasts.update((t) => t.filter((item) => item.id !== id));
}
