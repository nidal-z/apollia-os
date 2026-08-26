// Code Connect · Apollia OS · Sheet  (Figma node 359:294)
// Source: crates/apollia-desktop/ui/src/lib/components/ui/sheet/Sheet.svelte
// Generated from figma/manifest.json. Do not edit by hand: run the generator
// again after any rebuild of the component in Figma.
// Apollia UI is Svelte; this uses the @figma/code-connect HTML parser as the
// publish-ready Svelte stand-in. See ../README.md for plan/status.
import figma, { html } from "@figma/code-connect/html"

figma.connect("<DS_FILE>?node-id=359-294", {
  props: {
    width: figma.enum("Width", { "sm": "sm", "md": "md", "lg": "lg" }),
    side: figma.enum("Side", { "left": "left", "right": "right" }),
  },
  example: (props) => html`<Sheet width=${props.width} side=${props.side}></Sheet>`,
})
