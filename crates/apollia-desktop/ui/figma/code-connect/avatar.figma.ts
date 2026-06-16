// Code Connect · Apollia OS · Avatar  (Figma node 36:22)
// Source: crates/apollia-desktop/ui/src/lib/components/ui/avatar/Avatar.svelte
// Apollia UI is Svelte; this uses the @figma/code-connect HTML parser as the
// publish-ready Svelte stand-in. See ../README.md for plan/status.
import figma, { html } from "@figma/code-connect/html"

figma.connect("<DS_FILE>?node-id=36-22", {
  props: {
    size: figma.enum("Size", { "xs": "xs", "sm": "sm", "md": "md", "lg": "lg", "xl": "xl" }),
    ring: figma.boolean("Ring"),
  },
  example: (props) => html`<Avatar size=${props.size} ring=${props.ring}></Avatar>`,
})
