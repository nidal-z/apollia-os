/** Apollia OS design tokens — Warm Glassmorphism with brand blue/violet identity. */
export const tokens = {
  brand: {
    primary: { hex: "#3435f5", hsl: "240 91% 58%" },
    secondary: { hex: "#7c5fd6", hsl: "260 60% 61%" },
    accent: { hex: "#3435f5", hsl: "240 91% 58%" },
    background: { hex: "#f5f3eb", hsl: "42 30% 95%" },
    darkBackground: { hex: "#131320", hsl: "240 12% 9%" },
  },
  glass: {
    panel: "warm-cream/82 + brand gradient 2% + blur-2xl",
    card: "white/72 + brand gradient 1.8% + blur-xl + multi-layer shadow with brand glow",
    cardHover: "elevated shadow + translateY(-1px) + stronger brand glow",
    surface: "white/50 + brand gradient 1.2% + blur-md",
    inset: "primary/3% + blur-sm",
    border: "primary rgba(52,53,245,0.06) | dark secondary rgba(124,95,214,0.08)",
  },
  shadow: {
    card: "inset 0 0 0 1px rgba(255,255,255,0.5), 0 1px 2px rgba(0,0,0,0.03), 0 4px 16px -2px rgba(0,0,0,0.04), 0 12px 40px -8px rgba(52,53,245,0.04)",
    cardHover: "inset 0 0 0 1px rgba(255,255,255,0.6), 0 2px 4px rgba(0,0,0,0.04), 0 8px 24px -4px rgba(0,0,0,0.06), 0 20px 60px -12px rgba(52,53,245,0.08)",
  },
  radius: {
    sm: "0.375rem",
    md: "0.5rem",
    lg: "0.75rem",
    xl: "1rem",
    "2xl": "1.25rem",
  },
  spacing: {
    page: "2rem",
    section: "1.5rem",
    card: "1.25rem",
  },
} as const;
