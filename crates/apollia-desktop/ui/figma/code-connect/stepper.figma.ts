// Code Connect · Apollia OS · Stepper  (Figma node 373:1089)
// Source: crates/apollia-desktop/ui/src/lib/components/ui/stepper/Stepper.svelte
// Generated from figma/manifest.json. Do not edit by hand: run the generator
// again after any rebuild of the component in Figma.
// Apollia UI is Svelte; this uses the @figma/code-connect HTML parser as the
// publish-ready Svelte stand-in. See ../README.md for plan/status.
import figma, { html } from "@figma/code-connect/html"

figma.connect("<DS_FILE>?node-id=373-1089", {
  props: {
    orientation: figma.enum("Orientation", { "horizontal": "horizontal", "vertical": "vertical" }),
  },
  example: (props) => html`<Stepper orientation=${props.orientation}></Stepper>`,
})
