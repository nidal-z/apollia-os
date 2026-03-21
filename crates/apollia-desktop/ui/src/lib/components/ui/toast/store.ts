import { writable } from "svelte/store";

interface ToastItem {
  id: string;
  message: string;
  variant: "success" | "error" | "info";
}

export const toasts = writable<ToastItem[]>([]);

let counter = 0;

export function addToast(message: string, variant: ToastItem["variant"] = "info") {
  const id = `toast-${++counter}`;
  toasts.update((t) => [...t, { id, message, variant }]);

  setTimeout(() => removeToast(id), 4000);
}

export function removeToast(id: string) {
  toasts.update((t) => t.filter((item) => item.id !== id));
}
