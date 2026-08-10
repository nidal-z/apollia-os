// Code Connect · Apollia OS · Input  (Figma node 356:89)
// Source: crates/apollia-desktop/ui/src/lib/components/ui/input/Input.svelte
// Genere depuis figma/manifest.json. Ne pas editer a la main : reexecuter le
// generateur apres toute reconstruction du composant dans Figma.
// Apollia UI is Svelte; this uses the @figma/code-connect HTML parser as the
// publish-ready Svelte stand-in. See ../README.md for plan/status.
import figma, { html } from "@figma/code-connect/html"

figma.connect("<DS_FILE>?node-id=356-89", {
  props: {
    size: figma.enum("Size", { "sm": "sm", "default": "default", "lg": "lg" }),
    state: figma.enum("State", { "rest": "rest", "focus": "focus", "disabled": "disabled" }),
    icon: figma.enum("Icon", { "false": "false", "true": "true" }),
  },
  example: (props) => html`<Input size=${props.size} state=${props.state} icon=${props.icon}></Input>`,
})
