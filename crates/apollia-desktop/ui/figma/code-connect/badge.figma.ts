// Code Connect · Apollia OS · Badge  (Figma node 349:314)
// Source: crates/apollia-desktop/ui/src/lib/components/ui/badge/Badge.svelte
// Genere depuis figma/manifest.json. Ne pas editer a la main : reexecuter le
// generateur apres toute reconstruction du composant dans Figma.
// Apollia UI is Svelte; this uses the @figma/code-connect HTML parser as the
// publish-ready Svelte stand-in. See ../README.md for plan/status.
import figma, { html } from "@figma/code-connect/html"

figma.connect("<DS_FILE>?node-id=349-314", {
  props: {
    variant: figma.enum("Variant", { "neutral": "neutral", "primary": "primary", "success": "success", "warning": "warning", "danger": "danger", "info": "info", "outline": "outline", "gradient-primary": "gradient-primary", "gradient-success": "gradient-success", "gradient-warning": "gradient-warning", "gradient-destructive": "gradient-destructive" }),
    size: figma.enum("Size", { "sm": "sm", "md": "md", "lg": "lg" }),
    outline: figma.enum("Outline", { "false": "false", "true": "true" }),
  },
  example: (props) => html`<Badge variant=${props.variant} size=${props.size} outline=${props.outline}></Badge>`,
})
