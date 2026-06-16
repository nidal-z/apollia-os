// Code Connect · Apollia OS · Separator  (Figma node 37:26)
// Source: crates/apollia-desktop/ui/src/lib/components/ui/separator/Separator.svelte
// Apollia UI is Svelte; this uses the @figma/code-connect HTML parser as the
// publish-ready Svelte stand-in. See ../README.md for plan/status.
import figma, { html } from "@figma/code-connect/html"

figma.connect("<DS_FILE>?node-id=37-26", {
  props: {
    variant: figma.enum("Variant", { "solid": "solid", "subtle": "subtle", "elevated": "elevated", "fade": "fade", "inline": "inline", "dashed": "dashed" }),
    orientation: figma.enum("Orientation", { "horizontal": "horizontal", "vertical": "vertical" }),
  },
  example: (props) => html`<Separator variant=${props.variant} orientation=${props.orientation}></Separator>`,
})
