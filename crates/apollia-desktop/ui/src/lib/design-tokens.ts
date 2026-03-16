/** Apollia OS design tokens — single source of truth for spacing, radii, and shadows. */
export const tokens = {
  radius: {
    sm: "0.375rem",
    md: "0.5rem",
    lg: "0.75rem",
    xl: "1rem",
  },
  shadow: {
    sm: "0 1px 2px 0 rgb(0 0 0 / 0.05)",
    md: "0 4px 6px -1px rgb(0 0 0 / 0.1)",
  },
  spacing: {
    page: "1.5rem",
    section: "1rem",
    card: "1rem",
  },
} as const;
