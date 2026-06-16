// Code Connect · Apollia OS · ActionMenu  (Figma node 62:19)
// Source: crates/apollia-desktop/ui/src/lib/components/ui/action-menu/ActionMenu.svelte
// Apollia UI is Svelte; this uses the @figma/code-connect HTML parser as the
// publish-ready Svelte stand-in. See ../README.md for plan/status.
import figma, { html } from "@figma/code-connect/html"

figma.connect("<DS_FILE>?node-id=62-19", {
  props: {
    state: figma.enum("State", { "closed": "closed", "open": "open" }),
  },
  example: (props) => html`<ActionMenu state=${props.state}></ActionMenu>`,
})
