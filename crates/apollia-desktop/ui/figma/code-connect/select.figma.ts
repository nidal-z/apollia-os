// Code Connect · Apollia OS · Select  (Figma node 28:29)
// Source: crates/apollia-desktop/ui/src/lib/components/ui/select/Select.svelte
// Apollia UI is Svelte; this uses the @figma/code-connect HTML parser as the
// publish-ready Svelte stand-in. See ../README.md for plan/status.
import figma, { html } from "@figma/code-connect/html"

figma.connect("<DS_FILE>?node-id=28-29", {
  props: {
    size: figma.enum("Size", { "sm": "sm", "default": "default", "lg": "lg" }),
    state: figma.enum("State", { "rest": "rest", "focus": "focus", "disabled": "disabled" }),
  },
  example: (props) => html`<Select size=${props.size} state=${props.state}></Select>`,
})
