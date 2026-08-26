// Code Connect · Apollia OS · Popover  (Figma node 360:315)
// Source: crates/apollia-desktop/ui/src/lib/components/ui/popover/Popover.svelte
// Generated from figma/manifest.json. Do not edit by hand: run the generator
// again after any rebuild of the component in Figma.
// Apollia UI is Svelte; this uses the @figma/code-connect HTML parser as the
// publish-ready Svelte stand-in. See ../README.md for plan/status.
import figma, { html } from "@figma/code-connect/html"

figma.connect("<DS_FILE>?node-id=360-315", {
  props: {
    side: figma.enum("Side", { "top": "top", "right": "right", "bottom": "bottom", "left": "left" }),
    align: figma.enum("Align", { "start": "start", "center": "center", "end": "end" }),
  },
  example: (props) => html`<Popover side=${props.side} align=${props.align}></Popover>`,
})
