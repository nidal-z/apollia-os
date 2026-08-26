// Code Connect · Apollia OS · Checkbox  (Figma node 356:185)
// Source: crates/apollia-desktop/ui/src/lib/components/ui/checkbox/Checkbox.svelte
// Generated from figma/manifest.json. Do not edit by hand: run the generator
// again after any rebuild of the component in Figma.
// Apollia UI is Svelte; this uses the @figma/code-connect HTML parser as the
// publish-ready Svelte stand-in. See ../README.md for plan/status.
import figma, { html } from "@figma/code-connect/html"

figma.connect("<DS_FILE>?node-id=356-185", {
  props: {
    state: figma.enum("State", { "unchecked": "unchecked", "checked": "checked", "focus": "focus", "disabled-unchecked": "disabled-unchecked", "disabled-checked": "disabled-checked", "loading": "loading" }),
  },
  example: (props) => html`<Checkbox state=${props.state}></Checkbox>`,
})
