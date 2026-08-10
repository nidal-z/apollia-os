// Code Connect · Apollia OS · DatePicker  (Figma node 376:1157)
// Source: crates/apollia-desktop/ui/src/lib/components/ui/date-picker/DatePicker.svelte
// Genere depuis figma/manifest.json. Ne pas editer a la main : reexecuter le
// generateur apres toute reconstruction du composant dans Figma.
// Apollia UI is Svelte; this uses the @figma/code-connect HTML parser as the
// publish-ready Svelte stand-in. See ../README.md for plan/status.
import figma, { html } from "@figma/code-connect/html"

figma.connect("<DS_FILE>?node-id=376-1157", {
  props: {
    state: figma.enum("State", { "field": "field", "open": "open" }),
  },
  example: (props) => html`<DatePicker state=${props.state}></DatePicker>`,
})
