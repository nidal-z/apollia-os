/**
 * Motion tokens.
 *
 * Central presets for `svelte/motion` springs and for transition durations.
 * Durations mirror the `--motion-*` CSS custom properties declared in
 * `src/app.css`, so JS and CSS animations stay in lockstep.
 *
 * Usage:
 *   import { spring, duration, easing } from "$lib/design/motion";
 *   const translate = spring({ y: 0 }, spring.default);
 *
 * `prefersReducedMotion` returns true when the OS / browser requests reduced
 * motion. In that case, consumers should either skip animations or set
 * `duration: 0`. Spring presets do not auto-adapt - caller responsibility.
 */

/** Spring configs - tuned to feel tactile without overshooting. */
export const spring = {
  /** Default for hover lifts, list item translate/scale (stiff, low bounce). */
  default: { stiffness: 0.22, damping: 0.24 },
  /** Slower, softer - large panels, sheets sliding in. */
  gentle: { stiffness: 0.12, damping: 0.32 },
  /** Short, punchy - press feedback, micro-reactions. */
  snappy: { stiffness: 0.38, damping: 0.28 },
} as const;

/**
 * Framer-style spring parameters (physical stiffness / damping).
 *
 * Svelte's `spring()` uses normalised 0-1 stiffness/damping values (the numbers
 * above). We also expose the CSS-friendly "motion design" scale for
 * documentation and for libraries that expect raw spring physics - these are
 * the values the story references (`stiffness: 220, damping: 24`).
 */
export const springPhysics = {
  default: { stiffness: 220, damping: 24, mass: 1 },
  gentle: { stiffness: 120, damping: 32, mass: 1 },
  snappy: { stiffness: 380, damping: 28, mass: 0.8 },
} as const;

/** Transition durations (ms) - keep in sync with `--motion-*` in app.css. */
export const duration = {
  /** 120ms - micro-interactions (hover color, icon pop). */
  fast: 120,
  /** 200ms - page entrance, list slide, buttons. */
  base: 200,
  /** 320ms - overlays, drawers, large surfaces. */
  slow: 320,
} as const;

/** Easing curves - `apple` is the Tailwind-registered cubic-bezier. */
export const easing = {
  /** cubic-bezier(0.2, 0, 0, 1) - decelerate, default for entrances. */
  apple: [0.2, 0, 0, 1] as const,
  /** cubic-bezier(0.4, 0, 0.2, 1) - standard ease-in-out. */
  standard: [0.4, 0, 0.2, 1] as const,
  /** cubic-bezier(0.4, 0, 1, 1) - accelerate, for exits. */
  accel: [0.4, 0, 1, 1] as const,
} as const;

/**
 * Returns true when the user requests reduced motion via OS / browser
 * settings. SSR-safe (returns false when `window` is undefined).
 */
export function prefersReducedMotion(): boolean {
  if (globalThis.window === undefined || typeof globalThis.matchMedia !== "function") {
    return false;
  }
  return globalThis.matchMedia("(prefers-reduced-motion: reduce)").matches;
}

/**
 * Wrap a transition params object so durations collapse to 0 when the user
 * requests reduced motion. Handy for `transition:fly={rm({ y: 8, duration: 200 })}`.
 */
export function rm<T extends { duration?: number; delay?: number }>(params: T): T {
  if (!prefersReducedMotion()) return params;
  return { ...params, duration: 0, delay: 0 };
}

export const motion = {
  spring,
  springPhysics,
  duration,
  easing,
  prefersReducedMotion,
  rm,
} as const;
