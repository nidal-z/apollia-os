// Code Connect · Apollia OS · TimePicker  (Figma node 63:108)
// Source: crates/apollia-desktop/ui/src/lib/components/ui/date-picker/TimePicker.svelte
// Apollia UI is Svelte; this uses the @figma/code-connect HTML parser as the
// publish-ready Svelte stand-in. See ../README.md for plan/status.
import figma, { html } from "@figma/code-connect/html"

figma.connect("<DS_FILE>?node-id=63-108", {
  props: {
    state: figma.enum("State", { "field": "field", "open": "open" }),
  },
  example: (props) => html`<TimePicker state=${props.state}></TimePicker>`,
})
