// Code Connect · Apollia OS · Button  (Figma node 355:1400)
// Source: crates/apollia-desktop/ui/src/lib/components/ui/button/Button.svelte
// Generated from figma/manifest.json. Do not edit by hand: run the generator
// again after any rebuild of the component in Figma.
// Apollia UI is Svelte; this uses the @figma/code-connect HTML parser as the
// publish-ready Svelte stand-in. See ../README.md for plan/status.
import figma, { html } from "@figma/code-connect/html"

figma.connect("<DS_FILE>?node-id=355-1400", {
  props: {
    variant: figma.enum("Variant", { "default": "default", "primary-solid": "primary-solid", "primary-gradient": "primary-gradient", "destructive": "destructive", "success": "success", "outline": "outline", "secondary": "secondary", "ghost": "ghost", "link": "link", "elevated": "elevated", "soft": "soft" }),
    size: figma.enum("Size", { "default": "default", "sm": "sm", "lg": "lg", "icon": "icon", "icon-sm": "icon-sm", "auto": "auto" }),
    state: figma.enum("State", { "rest": "rest", "hover": "hover", "active": "active", "focus": "focus", "disabled": "disabled", "loading": "loading" }),
  },
  example: (props) => html`<Button variant=${props.variant} size=${props.size} state=${props.state}></Button>`,
})
