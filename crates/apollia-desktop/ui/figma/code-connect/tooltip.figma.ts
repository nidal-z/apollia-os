// Code Connect · Apollia OS · Tooltip  (Figma node 47:18)
// Source: crates/apollia-desktop/ui/src/lib/components/ui/tooltip/Tooltip.svelte
// Apollia UI is Svelte; this uses the @figma/code-connect HTML parser as the
// publish-ready Svelte stand-in. See ../README.md for plan/status.
import figma, { html } from "@figma/code-connect/html"

figma.connect("<DS_FILE>?node-id=47-18", {
  props: {
    side: figma.enum("Side", { "top": "top", "right": "right", "bottom": "bottom", "left": "left" }),
  },
  example: (props) => html`<Tooltip side=${props.side}></Tooltip>`,
})
