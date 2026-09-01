import type { Config } from "tailwindcss";
import defaultTheme from "tailwindcss/defaultTheme";

const config: Config = {
  darkMode: "class",
  content: ["./src/**/*.{html,js,svelte,ts}"],
  theme: {
    // Canonical responsive breakpoints for Apollia OS desktop.
    // Source of truth: src/lib/design/breakpoints.md.
    // The "operator mobile" threshold is 375 px (iPhone SE) - xs has to stay usable at that width.
    screens: {
      xs: "375px",
      sm: "640px",
      md: "768px",
      lg: "1024px",
      xl: "1280px",
      "2xl": "1536px",
    },
    extend: {
      // Every name here must be bundled by src/main.ts or come from the
      // system. A second rank that is itself bundled can never be reached:
      // the first rank cannot fail to load. No serif stack is declared, so
      // the serif utility resolves to Tailwind's system default rather than
      // to a typeface the installer does not carry.
      fontFamily: {
        sans: ["Inter Tight", ...defaultTheme.fontFamily.sans],
        mono: ["JetBrains Mono", "IBM Plex Mono", "ui-monospace", "monospace"],
      },
      // Display type scale - hero titles & empty-state headlines.
      // Values use `clamp(min, preferred, max)` so the scale breathes between the
      // xs (375 px) and xl (1280 px) breakpoints without extra media queries.
      fontSize: {
        "display-xl": [
          "var(--text-display-xl)",
          { lineHeight: "1.05", letterSpacing: "-0.03em", fontWeight: "700" },
        ],
        "display-lg": [
          "var(--text-display-lg)",
          { lineHeight: "1.1", letterSpacing: "-0.025em", fontWeight: "700" },
        ],
        "display-md": [
          "var(--text-display-md)",
          { lineHeight: "1.15", letterSpacing: "-0.02em", fontWeight: "600" },
        ],
        "display-sm": [
          "var(--text-display-sm)",
          { lineHeight: "1.2", letterSpacing: "-0.015em", fontWeight: "600" },
        ],

        // ── Reading scale ──────────────────────────────────────────────
        // The tiers below the display scale: section/card headings, body
        // copy, labels, and the fine print. Fixed px (not clamp) so dense
        // product chrome stays predictable. Tuple form matches the display
        // tiers: [size, { lineHeight, letterSpacing, fontWeight? }].
        //
        // Every size comes from a `--text-*` token of src/app.css, which is
        // the single place a size is written: a component that sizes text
        // inside a style block reads the same token, and cannot drift from
        // the class it replaces.
        //
        // The px map of the scale, six rungs:
        //   10.5px  → overline, the uppercase role a badge carries, plus
        //             micro-xs / micro-sm for a key cap inside a line
        //   11px    → micro (.tb-card-top, .section-meta), micro-lg
        //             (.tb-iolabel, .tb-pill) - the lowercase chips
        //   12.5px  → caption (.tb-badge, .tb-metric, .chat-ft-meta, .tb-fmeta)
        //             - the meta line under a title
        //   13px    → caption-lg (.tb-grep-file), body-xs / label-sm (.tb-chip,
        //             .tb-code, .tb-term, .tb-preview, .tb-grep-row), code-sm
        //             (.tb-frow, .tb-step, .chat-ft-head, .tb-card-body,
        //             .tb-extract) - secondary lines and code
        //   14px    → body-sm / label-md (.tb-extract-title), body-md - titles
        //             and body copy
        //   16px    → body-lg / heading-sm
        //
        // The uppercase rung sits three rungs under the title rather than one,
        // and that gap is deliberate. Set in caps at semibold, a label reads
        // larger than its nominal size, because what the eye compares is the
        // cap height against the neighbour's x-height. Rendered side by side
        // against the built stylesheet, a 12.5px badge weighed as much as the
        // 14px name it annotates; at 10.5px it reads as the annotation it is.
        //
        // The floor used to sit at 9px, and 95% of the 1569 sizing sites of
        // src/ resolved below 14px, 612 of them to the 11px `caption` alone,
        // against 17 sites on the 14px body tier. Twelve tiers shared the band
        // between 9 and 13px, where no reader perceives the half-pixel steps
        // that separate them.
        //
        // Lifting that floor alone was not enough, and a test round said so: a
        // chip rose 25% while the title beside it rose nothing, so a bold
        // uppercase badge came to read as large as the entity it annotates, and
        // a key cap as large as the sentence holding it. What a scale owes a
        // reader is the ratio, not the size. The rungs above the chips were
        // lifted with them, and a chip now sits one rung under the meta line
        // and two under the title.
        //
        // The five chrome tiers carry a size and nothing else. They were
        // added for the sizes a measure of the tree found at least five times
        // with no palier (10px at 159 sites, 11.5 at 29, 9 at 22, 9.5 at 8),
        // and 10.5px as the plain size behind the `overline` role. A tuple
        // would have fought the `tracking-*`, `font-*` and `leading-*`
        // utilities those chrome sites already carry, since two utilities
        // writing the same property are resolved by stylesheet order rather
        // than by the order they appear in a class list.
        "heading-lg": [
          "var(--text-heading-lg)",
          { lineHeight: "1.3", letterSpacing: "-0.01em", fontWeight: "600" },
        ],
        "heading-md": [
          "var(--text-heading-md)",
          { lineHeight: "1.35", letterSpacing: "-0.01em", fontWeight: "600" },
        ],
        "heading-sm": [
          "var(--text-heading-sm)",
          { lineHeight: "1.4", letterSpacing: "-0.006em", fontWeight: "600" },
        ],
        "body-lg": [
          "var(--text-body-lg)",
          { lineHeight: "1.6", letterSpacing: "-0.003em" },
        ],
        "body-md": [
          "var(--text-body-md)",
          { lineHeight: "1.55", letterSpacing: "0em" },
        ],
        "body-sm": [
          "var(--text-body-sm)",
          { lineHeight: "1.5", letterSpacing: "0em" },
        ],
        "body-xs": [
          "var(--text-body-xs)",
          { lineHeight: "1.5", letterSpacing: "0em" },
        ],
        "label-md": [
          "var(--text-label-md)",
          { lineHeight: "1.2", letterSpacing: "0em", fontWeight: "500" },
        ],
        "label-sm": [
          "var(--text-label-sm)",
          { lineHeight: "1.2", letterSpacing: "0.005em", fontWeight: "500" },
        ],
        caption: [
          "var(--text-caption)",
          { lineHeight: "1.45", letterSpacing: "0.005em" },
        ],
        overline: [
          "var(--text-overline)",
          { lineHeight: "1.4", letterSpacing: "0.08em", fontWeight: "600" },
        ],
        "code-sm": [
          "var(--text-code-sm)",
          { lineHeight: "1.5", letterSpacing: "-0.01em" },
        ],
        // Counted in the tree when the five chrome rungs were added: micro
        // 159 sites, micro-lg 41, caption-lg 29, micro-xs 22, micro-sm 8.
        // The sizes under the five-site bar (9.6, 13.5, 14.5, 15, 16.5,
        // 28 px) round to the nearest rung instead of getting one of their
        // own.
        //
        // These five are bare sizes, not tuples, and that is deliberate. A
        // tuple emits `line-height` alongside `font-size`, so replacing
        // `text-[10px]` with a tuple rung would move some 160 call sites by
        // a fraction of a line box for a line-height nobody chose. At these
        // sizes the line box belongs to the surface, which sets it with
        // `leading-*`. The rungs above keep their tuples: their
        // line-heights are designed values. `micro-lg` and `overline` share
        // a size and differ in style, the same arrangement `body-sm` and
        // `label-md` have had since the scale was written.
        "caption-lg": "var(--text-caption-lg)",
        "micro-lg": "var(--text-micro-lg)",
        micro: "var(--text-micro)",
        "micro-sm": "var(--text-micro-sm)",
        "micro-xs": "var(--text-micro-xs)",

        // Tailwind's own keys, given the tiers above rather than its defaults.
        //
        // A measure of this tree found 1520 uses of the scale above and 529 of
        // Tailwind's default one, `text-sm` at 260 sites and `text-xs` at 234.
        // The sizes agreed, which is why nothing looked plainly wrong; the
        // leading did not. Tailwind puts `text-sm` on a 1.25rem line and
        // `text-xs` on a 1rem line, where `body-md` and `body-xs` put the same
        // sizes on 1.55 and 1.5. Five hundred places rendered at the right
        // size with the wrong rhythm, which reads as untidy without any number
        // being visibly false.
        //
        // Aliased rather than swept across 529 call sites: both vocabularies
        // now emit the same declarations, so a component keeps the name it was
        // written with and a page stops mixing two rhythms.
        //
        // The three body tiers only, covering 512 of the 529. `lg`, `xl` and
        // `2xl` are left on Tailwind's defaults at seventeen sites between
        // them: the tiers that would receive them are heading tiers carrying
        // `fontWeight: 600`, and an alias that silently bolds seventeen places
        // is a different change from one that fixes their leading.
        xs: [
          "var(--text-body-xs)",
          { lineHeight: "1.5", letterSpacing: "0em" },
        ],
        sm: [
          "var(--text-body-md)",
          { lineHeight: "1.55", letterSpacing: "0em" },
        ],
        base: [
          "var(--text-body-lg)",
          { lineHeight: "1.6", letterSpacing: "-0.003em" },
        ],
      },
      // The `--z-*` stack of app.css, reachable as a class, so a stacking
      // decision is taken by name rather than by a number a reader has to
      // compare with every other number in the tree. Tailwind ships no
      // `zIndex` mapping by default, so `z-30` and `z-40` were raw numbers
      // that happened to agree with a token rather than reads of it. Values
      // are unchanged; only the name is new. `src/components/` and
      // `src/lib/` still write rungs that have no token (20, 64, 80, 90,
      // 91); they join this table with their file.
      zIndex: {
        sticky: "var(--z-sticky)",
        "drawer-backdrop": "var(--z-drawer-backdrop)",
        drawer: "var(--z-drawer)",
        backdrop: "var(--z-backdrop)",
        toast: "var(--z-toast)",
        overlay: "var(--z-overlay)",
        tooltip: "var(--z-tooltip)",
      },
      spacing: {
        // Panel widths the chat shell writes; Tailwind's scale stops at 64
        // (16rem) then jumps to 72, so 280px and 380px had no step.
        70: "17.5rem", // 280px
        95: "23.75rem", // 380px
      },
      colors: {
        border: "hsl(var(--border))",
        input: "hsl(var(--input))",
        ring: "hsl(var(--ring))",
        background: "hsl(var(--background))",
        // The scrim behind a drawer or a modal. Its own token rather than a
        // role tinted by opacity: it is warm in the light theme and near
        // black in the dark one, which no `<role>/30` can express.
        backdrop: "var(--backdrop)",
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
