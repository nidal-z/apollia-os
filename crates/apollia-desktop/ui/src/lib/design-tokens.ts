/** Apollia OS design tokens — single source of truth for brand, warm glassmorphism, spacing, radii, and shadows. */
export const tokens = {
  brand: {
    primary: { hex: "#3435f5", hsl: "240 91% 58%" },
    secondary: { hex: "#7c5fd6", hsl: "260 60% 61%" },
    accent: { hex: "#64d971", hsl: "130 58% 62%" },
    background: { hex: "#f5f4ed", hsl: "48 33% 95%" },
    darkBackground: { hex: "#0f0f1a", hsl: "240 15% 8%" },
  },
  glass: {
    panel: "cream/75 + primary gradient 3% + blur-2xl",
    card: "white/65 + primary gradient 2.5% + blur-xl + colored shadow",
    surface: "white/45 + primary gradient 2% + blur-md",
    inset: "primary/4% + blur-sm",
    border: "primary rgba(52,53,245,0.10) | dark secondary rgba(124,95,214,0.12)",
    hover: "primary rgba(52,53,245,0.04) | dark secondary rgba(124,95,214,0.06)",
  },
  shadow: {
    card: "0 4px 24px -4px rgba(52,53,245,0.07), 0 1px 3px rgba(0,0,0,0.04)",
    cardHover: "0 8px 40px -8px rgba(52,53,245,0.13)",
    button: "0 4px 16px -2px rgba(52,53,245,0.35)",
    buttonHover: "0 6px 24px -2px rgba(52,53,245,0.45)",
  },
  radius: {
    sm: "0.375rem",
    md: "0.5rem",
    lg: "0.75rem",
    xl: "1rem",
  },
  spacing: {
    page: "1.5rem",
    section: "1rem",
    card: "1rem",
  },
} as const;
