import type { Config } from "tailwindcss";
import defaultTheme from "tailwindcss/defaultTheme";

const config: Config = {
  darkMode: "class",
  content: ["./src/**/*.{html,js,svelte,ts}"],
  theme: {
    // Canonical responsive breakpoints for Apollia OS desktop.
    // Source of vérité : src/lib/design/breakpoints.md.
    // Seuil "operator mobile" = 375 px (iPhone SE) - xs doit rester fonctionnel à cette largeur.
    screens: {
      xs: "375px",
      sm: "640px",
      md: "768px",
      lg: "1024px",
      xl: "1280px",
      "2xl": "1536px",
    },
    extend: {
      fontFamily: {
        sans: ["Inter Tight", "Inter", ...defaultTheme.fontFamily.sans],
        serif: ["Instrument Serif", "Cormorant Garamond", "Georgia", "serif"],
        mono: ["JetBrains Mono", "IBM Plex Mono", "ui-monospace", "monospace"],
      },
      // Display type scale - hero titles & empty-state headlines.
      // Values use `clamp(min, preferred, max)` so the scale breathes between the
      // xs (375 px) and xl (1280 px) breakpoints without extra media queries.
      fontSize: {
        "display-xl": [
          "clamp(3rem, 2rem + 3.2vw, 4.5rem)",
          { lineHeight: "1.05", letterSpacing: "-0.03em", fontWeight: "700" },
        ],
        "display-lg": [
          "clamp(2.25rem, 1.5rem + 2.2vw, 3.25rem)",
          { lineHeight: "1.1", letterSpacing: "-0.025em", fontWeight: "700" },
        ],
        "display-md": [
          "clamp(1.75rem, 1.25rem + 1.4vw, 2.25rem)",
          { lineHeight: "1.15", letterSpacing: "-0.02em", fontWeight: "600" },
        ],
        "display-sm": [
          "1.5rem",
          { lineHeight: "1.2", letterSpacing: "-0.015em", fontWeight: "600" },
        ],
      },
      colors: {
        border: "hsl(var(--border))",
        input: "hsl(var(--input))",
        ring: "hsl(var(--ring))",
        background: "hsl(var(--background))",
        foreground: "hsl(var(--foreground))",
        primary: {
          DEFAULT: "hsl(var(--primary))",
          foreground: "hsl(var(--primary-foreground))",
          hover: "hsl(var(--primary-hover))",
          active: "hsl(var(--primary-active))",
        },
        secondary: {
          DEFAULT: "hsl(var(--secondary))",
          foreground: "hsl(var(--secondary-foreground))",
        },
        destructive: {
          DEFAULT: "hsl(var(--destructive))",
          foreground: "hsl(var(--destructive-foreground))",
        },
        muted: {
          DEFAULT: "hsl(var(--muted))",
          foreground: "hsl(var(--muted-foreground))",
        },
        accent: {
          DEFAULT: "hsl(var(--accent))",
          foreground: "hsl(var(--accent-foreground))",
        },
        card: {
          DEFAULT: "hsl(var(--card))",
          foreground: "hsl(var(--card-foreground))",
        },
        surface: {
          1: "hsl(var(--surface-1))",
          2: "hsl(var(--surface-2))",
          3: "hsl(var(--surface-3))",
        },
        info: {
          DEFAULT: "hsl(var(--info))",
          foreground: "hsl(var(--info-foreground))",
        },
        warning: {
          DEFAULT: "hsl(var(--warning))",
          foreground: "hsl(var(--warning-foreground))",
        },
        success: {
          DEFAULT: "hsl(var(--success))",
          foreground: "hsl(var(--success-foreground))",
        },
      },
      backdropBlur: {
        xs: "2px",
      },
      backgroundImage: {
        "primary-gradient":
          "linear-gradient(135deg, hsl(var(--primary-gradient-from)), hsl(var(--primary-gradient-to)))",
        "gradient-primary": "var(--gradient-primary)",
        "gradient-surface": "var(--gradient-surface)",
        "gradient-accent": "var(--gradient-accent)",
      },
      transitionDuration: {
        fast: "var(--motion-fast)",
        base: "var(--motion-base)",
        slow: "var(--motion-slow)",
      },
      animation: {
        "shimmer-slide": "shimmerSlide 1.4s linear infinite",
        "spinner-rotate": "spinnerRotate 0.7s linear infinite",
      },
      boxShadow: {
        "elev-0": "var(--shadow-elev-0)",
        "elev-1": "var(--shadow-elev-1)",
        "elev-2": "var(--shadow-elev-2)",
        "elev-3": "var(--shadow-elev-3)",
        "elev-4": "var(--shadow-elev-4)",
        "primary-sm": "var(--shadow-primary-sm)",
        "primary-md": "var(--shadow-primary-md)",
        "primary-lg": "var(--shadow-primary-lg)",
        "primary-xl": "var(--shadow-primary-xl)",
        "warm-focus": "var(--shadow-warm-focus)",
        "status-ok": "var(--shadow-status-ok)",
        "status-warn": "var(--shadow-status-warn)",
        "status-error": "var(--shadow-status-error)",
      },
      borderRadius: {
        lg: "var(--radius)",
        md: "calc(var(--radius) - 2px)",
        sm: "calc(var(--radius) - 4px)",
      },
      transitionTimingFunction: {
        apple: "cubic-bezier(0.2, 0, 0, 1)",
      },
    },
  },
  plugins: [],
};

export default config;
