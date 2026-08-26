// Code Connect · Apollia OS · ActionMenu  (Figma node 360:352)
// Source: crates/apollia-desktop/ui/src/lib/components/ui/action-menu/ActionMenu.svelte
// Generated from figma/manifest.json. Do not edit by hand: run the generator
// again after any rebuild of the component in Figma.
// Apollia UI is Svelte; this uses the @figma/code-connect HTML parser as the
// publish-ready Svelte stand-in. See ../README.md for plan/status.
import figma, { html } from "@figma/code-connect/html"

figma.connect("<DS_FILE>?node-id=360-352", {
  props: {
    state: figma.enum("State", { "closed": "closed", "open": "open" }),
  },
  example: (props) => html`<ActionMenu state=${props.state}></ActionMenu>`,
})
