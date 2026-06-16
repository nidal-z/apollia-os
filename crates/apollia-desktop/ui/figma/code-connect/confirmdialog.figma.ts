// Code Connect · Apollia OS · ConfirmDialog  (Figma node 62:47)
// Source: crates/apollia-desktop/ui/src/lib/components/ui/dialog/ConfirmDialog.svelte
// Apollia UI is Svelte; this uses the @figma/code-connect HTML parser as the
// publish-ready Svelte stand-in. See ../README.md for plan/status.
import figma, { html } from "@figma/code-connect/html"

figma.connect("<DS_FILE>?node-id=62-47", {
  props: {
    variant: figma.enum("Variant", { "info": "info", "warning": "warning", "destructive": "destructive" }),
  },
  example: (props) => html`<ConfirmDialog variant=${props.variant}></ConfirmDialog>`,
})
