// Code Connect · Apollia OS · Popover  (Figma node 52:38)
// Source: crates/apollia-desktop/ui/src/lib/components/ui/popover/Popover.svelte
// Apollia UI is Svelte; this uses the @figma/code-connect HTML parser as the
// publish-ready Svelte stand-in. See ../README.md for plan/status.
import figma, { html } from "@figma/code-connect/html"

figma.connect("<DS_FILE>?node-id=52-38", {
  props: {
    side: figma.enum("Side", { "top": "top", "right": "right", "bottom": "bottom", "left": "left" }),
  },
  example: (props) => html`<Popover side=${props.side}></Popover>`,
})
