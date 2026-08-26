// Code Connect · Apollia OS · FormField  (Figma node 346:78)
// Source: crates/apollia-desktop/ui/src/lib/components/ui/form-field/FormField.svelte
// Generated from figma/manifest.json. Do not edit by hand: run the generator
// again after any rebuild of the component in Figma.
// Apollia UI is Svelte; this uses the @figma/code-connect HTML parser as the
// publish-ready Svelte stand-in. See ../README.md for plan/status.
import figma, { html } from "@figma/code-connect/html"

figma.connect("<DS_FILE>?node-id=346-78", {
  props: {
    variant: figma.enum("Variant", { "hint": "hint", "error": "error", "required": "required", "optional": "optional", "inline": "inline" }),
  },
  example: (props) => html`<FormField variant=${props.variant}></FormField>`,
})
