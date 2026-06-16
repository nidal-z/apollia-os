// Code Connect · Apollia OS · Textarea  (Figma node 54:20)
// Source: crates/apollia-desktop/ui/src/lib/components/ui/textarea/Textarea.svelte
// Apollia UI is Svelte; this uses the @figma/code-connect HTML parser as the
// publish-ready Svelte stand-in. See ../README.md for plan/status.
import figma, { html } from "@figma/code-connect/html"

figma.connect("<DS_FILE>?node-id=54-20", {
  props: {
    size: figma.enum("Size", { "sm": "sm", "default": "default", "lg": "lg" }),
    state: figma.enum("State", { "rest": "rest", "focus": "focus", "disabled": "disabled" }),
  },
  example: (props) => html`<Textarea size=${props.size} state=${props.state}></Textarea>`,
})
