// Code Connect · Apollia OS · Command  (Figma node 60:44)
// Source: crates/apollia-desktop/ui/src/lib/components/ui/command/Command.svelte
// Apollia UI is Svelte; this uses the @figma/code-connect HTML parser as the
// publish-ready Svelte stand-in. See ../README.md for plan/status.
import figma, { html } from "@figma/code-connect/html"

figma.connect("<DS_FILE>?node-id=60-44", {
  props: {
    variant: figma.enum("Variant", { "default": "default", "empty": "empty" }),
  },
  example: (props) => html`<Command variant=${props.variant}></Command>`,
})
