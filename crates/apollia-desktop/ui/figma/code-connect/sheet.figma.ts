// Code Connect · Apollia OS · Sheet  (Figma node 46:38)
// Source: crates/apollia-desktop/ui/src/lib/components/ui/sheet/Sheet.svelte
// Apollia UI is Svelte; this uses the @figma/code-connect HTML parser as the
// publish-ready Svelte stand-in. See ../README.md for plan/status.
import figma, { html } from "@figma/code-connect/html"

figma.connect("<DS_FILE>?node-id=46-38", {
  props: {
    width: figma.enum("Width", { "sm": "sm", "md": "md", "lg": "lg" }),
    side: figma.enum("Side", { "left": "left", "right": "right" }),
  },
  example: (props) => html`<Sheet width=${props.width} side=${props.side}></Sheet>`,
})
