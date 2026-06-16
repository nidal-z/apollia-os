// Code Connect · Apollia OS · Skeleton  (Figma node 50:7)
// Source: crates/apollia-desktop/ui/src/lib/components/ui/skeleton/Skeleton.svelte
// Apollia UI is Svelte; this uses the @figma/code-connect HTML parser as the
// publish-ready Svelte stand-in. See ../README.md for plan/status.
import figma, { html } from "@figma/code-connect/html"

figma.connect("<DS_FILE>?node-id=50-7", {
  props: {
    variant: figma.enum("Variant", { "plain": "plain", "text": "text", "card": "card", "table-row": "table-row", "avatar": "avatar" }),
  },
  example: (props) => html`<Skeleton variant=${props.variant}></Skeleton>`,
})
