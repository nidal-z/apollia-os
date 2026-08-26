// Code Connect · Apollia OS · Banner  (Figma node 373:1027)
// Source: crates/apollia-desktop/ui/src/lib/components/ui/banner/Banner.svelte
// Generated from figma/manifest.json. Do not edit by hand: run the generator
// again after any rebuild of the component in Figma.
// Apollia UI is Svelte; this uses the @figma/code-connect HTML parser as the
// publish-ready Svelte stand-in. See ../README.md for plan/status.
import figma, { html } from "@figma/code-connect/html"

figma.connect("<DS_FILE>?node-id=373-1027", {
  props: {
    variant: figma.enum("Variant", { "info": "info", "success": "success", "warning": "warning", "destructive": "destructive", "neutral": "neutral" }),
    surface: figma.enum("Surface", { "edge": "edge", "card": "card" }),
  },
  example: (props) => html`<Banner variant=${props.variant} surface=${props.surface}></Banner>`,
})
