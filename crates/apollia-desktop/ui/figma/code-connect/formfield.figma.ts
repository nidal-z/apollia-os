// Code Connect · Apollia OS · FormField  (Figma node 55:32)
// Source: crates/apollia-desktop/ui/src/lib/components/ui/form-field/FormField.svelte
// Apollia UI is Svelte; this uses the @figma/code-connect HTML parser as the
// publish-ready Svelte stand-in. See ../README.md for plan/status.
import figma, { html } from "@figma/code-connect/html"

figma.connect("<DS_FILE>?node-id=55-32", {
  props: {
    variant: figma.enum("Variant", { "hint": "hint", "error": "error", "required": "required", "optional": "optional", "inline": "inline" }),
  },
  example: (props) => html`<FormField variant=${props.variant}></FormField>`,
})
