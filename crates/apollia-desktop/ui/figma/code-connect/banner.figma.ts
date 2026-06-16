// Code Connect · Apollia OS · Banner  (Figma node 40:32)
// Source: crates/apollia-desktop/ui/src/lib/components/ui/banner/Banner.svelte
// Apollia UI is Svelte; this uses the @figma/code-connect HTML parser as the
// publish-ready Svelte stand-in. See ../README.md for plan/status.
import figma, { html } from "@figma/code-connect/html"

figma.connect("<DS_FILE>?node-id=40-32", {
  props: {
    variant: figma.enum("Variant", { "info": "info", "success": "success", "warning": "warning", "destructive": "destructive", "neutral": "neutral" }),
    surface: figma.enum("Surface", { "edge": "edge", "card": "card" }),
  },
  example: (props) => html`<Banner variant=${props.variant} surface=${props.surface}></Banner>`,
})
