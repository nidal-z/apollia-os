// Code Connect · Apollia OS · DatePicker  (Figma node 63:90)
// Source: crates/apollia-desktop/ui/src/lib/components/ui/date-picker/DatePicker.svelte
// Apollia UI is Svelte; this uses the @figma/code-connect HTML parser as the
// publish-ready Svelte stand-in. See ../README.md for plan/status.
import figma, { html } from "@figma/code-connect/html"

figma.connect("<DS_FILE>?node-id=63-90", {
  props: {
    state: figma.enum("State", { "field": "field", "open": "open" }),
  },
  example: (props) => html`<DatePicker state=${props.state}></DatePicker>`,
})
