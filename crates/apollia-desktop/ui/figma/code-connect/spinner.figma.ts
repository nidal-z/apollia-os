// Code Connect · Apollia OS · Spinner  (Figma node 49:8)
// Source: crates/apollia-desktop/ui/src/lib/components/ui/progress/Spinner.svelte
// Apollia UI is Svelte; this uses the @figma/code-connect HTML parser as the
// publish-ready Svelte stand-in. See ../README.md for plan/status.
import figma, { html } from "@figma/code-connect/html"

figma.connect("<DS_FILE>?node-id=49-8", {
  props: {
    size: figma.enum("Size", { "sm": "sm", "md": "md", "lg": "lg" }),
  },
  example: (props) => html`<Spinner size=${props.size}></Spinner>`,
})
