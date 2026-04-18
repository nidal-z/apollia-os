/**
 * Design tokens v2 — ADR-077.
 *
 * Typed references to CSS variables declared in `src/app.css`. Prefer these
 * exports over raw CSS variable strings so that renames (or a future move to
 * a different backing store) surface at type-check time.
 *
 * Every token here resolves at runtime via `var()` — no values are duplicated.
 */

export const elevation = {
  /** Flat with a hint of rim light. Use for table rows, inline surfaces. */
  elev0: "var(--shadow-elev-0)",
  /** Cards, buttons at rest. */
  elev1: "var(--shadow-elev-1)",
  /** Raised cards, menus, popovers. */
  elev2: "var(--shadow-elev-2)",
  /** Overlays, sheets, hover state of raised cards. */
  elev3: "var(--shadow-elev-3)",
  /** Heroes, modals with spotlight. */
  elev4: "var(--shadow-elev-4)",
} as const;

export const primaryShadow = {
  /** Primary tint — buttons at rest. */
  sm: "var(--shadow-primary-sm)",
  /** Primary tint — raised CTAs. */
  md: "var(--shadow-primary-md)",
  /** Primary tint — hovered hero CTAs. */
  lg: "var(--shadow-primary-lg)",
  /** Primary tint — big featured buttons (Start agent, Send). */
  xl: "var(--shadow-primary-xl)",
} as const;

export const surface = {
  /** Most elevated card surface — modals, focused cards. */
  s1: "hsl(var(--surface-1))",
  /** Default card / panel body. */
  s2: "hsl(var(--surface-2))",
  /** Recessed / inset regions — empty states, diff viewers. */
  s3: "hsl(var(--surface-3))",
} as const;

export const glassBorder = {
  light: "hsl(var(--glass-border-light))",
  lightHover: "hsl(var(--glass-border-light-hover))",
  dark: "hsl(var(--glass-border-dark))",
  darkHover: "hsl(var(--glass-border-dark-hover))",
} as const;

export const backdrop = {
  /** Modal / sheet backdrop — bg-black/40 light, warm dark tint. */
  default: "var(--backdrop)",
  /** Lighter dim — popovers, quiet overlays. */
  subtle: "var(--backdrop-subtle)",
} as const;

export const primaryGradient = {
  /** 135deg gradient from primary → secondary. */
  css: "linear-gradient(135deg, hsl(var(--primary-gradient-from)), hsl(var(--primary-gradient-to)))",
} as const;

export type ElevationKey = keyof typeof elevation;
export type PrimaryShadowKey = keyof typeof primaryShadow;
export type SurfaceKey = keyof typeof surface;

export const tokens = {
  elevation,
  primaryShadow,
  surface,
  glassBorder,
  backdrop,
  primaryGradient,
} as const;

export default tokens;
