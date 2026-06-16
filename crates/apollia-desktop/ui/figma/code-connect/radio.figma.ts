// Code Connect · Apollia OS · Radio  (Figma node 56:20)
// Source: crates/apollia-desktop/ui/src/lib/components/ui/radio/RadioItem.svelte
// Apollia UI is Svelte; this uses the @figma/code-connect HTML parser as the
// publish-ready Svelte stand-in. See ../README.md for plan/status.
import figma, { html } from "@figma/code-connect/html"

figma.connect("<DS_FILE>?node-id=56-20", {
  props: {
    state: figma.enum("State", { "unchecked": "unchecked", "checked": "checked", "disabled-unchecked": "disabled-unchecked", "disabled-checked": "disabled-checked", "loading": "loading" }),
  },
  example: (props) => html`<Radio state=${props.state}></Radio>`,
})
