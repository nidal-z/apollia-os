// Code Connect · Apollia OS · Spinner  (Figma node 358:338)
// Source: crates/apollia-desktop/ui/src/lib/components/ui/progress/Spinner.svelte
// Generated from figma/manifest.json. Do not edit by hand: run the generator
// again after any rebuild of the component in Figma.
// Apollia UI is Svelte; this uses the @figma/code-connect HTML parser as the
// publish-ready Svelte stand-in. See ../README.md for plan/status.
import figma, { html } from "@figma/code-connect/html"

figma.connect("<DS_FILE>?node-id=358-338", {
  props: {
    variant: figma.enum("Variant", { "inline": "inline", "centered": "centered" }),
  },
  example: (props) => html`<Spinner variant=${props.variant}></Spinner>`,
})
