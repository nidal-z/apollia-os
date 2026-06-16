// Code Connect · Apollia OS · Checkbox  (Figma node 29:10)
// Source: crates/apollia-desktop/ui/src/lib/components/ui/checkbox/Checkbox.svelte
// Apollia UI is Svelte; this uses the @figma/code-connect HTML parser as the
// publish-ready Svelte stand-in. See ../README.md for plan/status.
import figma, { html } from "@figma/code-connect/html"

figma.connect("<DS_FILE>?node-id=29-10", {
  props: {
    state: figma.enum("State", { "unchecked": "unchecked", "checked": "checked", "disabled-unchecked": "disabled-unchecked", "disabled-checked": "disabled-checked", "loading": "loading" }),
  },
  example: (props) => html`<Checkbox state=${props.state}></Checkbox>`,
})
