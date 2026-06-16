// Code Connect · Apollia OS · Icon  (Figma node 61:20)
// Source: crates/apollia-desktop/ui/src/lib/components/ui/icon/Icon.svelte
// Apollia UI is Svelte; this uses the @figma/code-connect HTML parser as the
// publish-ready Svelte stand-in. See ../README.md for plan/status.
import figma, { html } from "@figma/code-connect/html"

figma.connect("<DS_FILE>?node-id=61-20", {
  props: {
    tone: figma.enum("Tone", { "default": "default", "muted": "muted", "primary": "primary", "success": "success", "warning": "warning", "danger": "danger" }),
  },
  example: (props) => html`<Icon tone=${props.tone}></Icon>`,
})
