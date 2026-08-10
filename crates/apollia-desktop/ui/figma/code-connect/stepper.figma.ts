// Code Connect · Apollia OS · Stepper  (Figma node 373:1089)
// Source: crates/apollia-desktop/ui/src/lib/components/ui/stepper/Stepper.svelte
// Genere depuis figma/manifest.json. Ne pas editer a la main : reexecuter le
// generateur apres toute reconstruction du composant dans Figma.
// Apollia UI is Svelte; this uses the @figma/code-connect HTML parser as the
// publish-ready Svelte stand-in. See ../README.md for plan/status.
import figma, { html } from "@figma/code-connect/html"

figma.connect("<DS_FILE>?node-id=373-1089", {
  props: {
    orientation: figma.enum("Orientation", { "horizontal": "horizontal", "vertical": "vertical" }),
  },
  example: (props) => html`<Stepper orientation=${props.orientation}></Stepper>`,
})
