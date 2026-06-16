// Code Connect · Apollia OS · Button  (Figma node 23:2)
// Source: crates/apollia-desktop/ui/src/lib/components/ui/button/Button.svelte
// Apollia UI is Svelte; this uses the @figma/code-connect HTML parser as the
// publish-ready Svelte stand-in. See ../README.md for plan/status.
import figma, { html } from "@figma/code-connect/html"

figma.connect("<DS_FILE>?node-id=23-2", {
  props: {
    variant: figma.enum("Variant", { "default": "default", "primary-solid": "primary-solid", "primary-gradient": "primary-gradient", "destructive": "destructive", "success": "success", "outline": "outline", "secondary": "secondary", "ghost": "ghost", "link": "link", "elevated": "elevated", "soft": "soft" }),
    size: figma.enum("Size", { "default": "default", "sm": "sm", "lg": "lg", "icon": "icon", "icon-sm": "icon-sm", "auto": "auto" }),
    state: figma.enum("State", { "rest": "rest", "hover": "hover", "active": "active", "disabled": "disabled", "loading": "loading" }),
  },
  example: (props) => html`<Button variant=${props.variant} size=${props.size} state=${props.state}></Button>`,
})
