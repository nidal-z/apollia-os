// Code Connect · Apollia OS · DataTable  (Figma node 59:92)
// Source: crates/apollia-desktop/ui/src/lib/components/ui/data-table/DataTable.svelte
// Apollia UI is Svelte; this uses the @figma/code-connect HTML parser as the
// publish-ready Svelte stand-in. See ../README.md for plan/status.
import figma, { html } from "@figma/code-connect/html"

figma.connect("<DS_FILE>?node-id=59-92", {
  props: {
    variant: figma.enum("Variant", { "default": "default", "loading": "loading", "empty": "empty" }),
  },
  example: (props) => html`<DataTable variant=${props.variant}></DataTable>`,
})
