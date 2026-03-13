import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

/** Merge Tailwind classes with clsx and tailwind-merge (shadcn-svelte pattern). */
export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}
