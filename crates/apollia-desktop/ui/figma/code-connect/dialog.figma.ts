// Code Connect · Apollia OS · Dialog  (Figma node 359:167)
// Source: crates/apollia-desktop/ui/src/lib/components/ui/dialog/Dialog.svelte
// Generated from figma/manifest.json. Do not edit by hand: run the generator
// again after any rebuild of the component in Figma.
// Apollia UI is Svelte; this uses the @figma/code-connect HTML parser as the
// publish-ready Svelte stand-in. See ../README.md for plan/status.
import figma, { html } from "@figma/code-connect/html"

figma.connect("<DS_FILE>?node-id=359-167", {
  props: {
    size: figma.enum("Size", { "sm": "sm", "md": "md", "lg": "lg", "xl": "xl" }),
  },
  example: (props) => html`<Dialog size=${props.size}></Dialog>`,
})
