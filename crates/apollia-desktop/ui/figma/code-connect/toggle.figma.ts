// Code Connect · Apollia OS · Toggle  (Figma node 30:24)
// Source: crates/apollia-desktop/ui/src/lib/components/ui/toggle/Toggle.svelte
// Apollia UI is Svelte; this uses the @figma/code-connect HTML parser as the
// publish-ready Svelte stand-in. See ../README.md for plan/status.
import figma, { html } from "@figma/code-connect/html"

figma.connect("<DS_FILE>?node-id=30-24", {
  props: {
    size: figma.enum("Size", { "sm": "sm", "default": "default" }),
    state: figma.enum("State", { "off": "off", "on": "on", "disabled-off": "disabled-off", "disabled-on": "disabled-on", "loading": "loading" }),
  },
  example: (props) => html`<Toggle size=${props.size} state=${props.state}></Toggle>`,
})
