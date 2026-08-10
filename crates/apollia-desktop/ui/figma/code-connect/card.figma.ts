// Code Connect · Apollia OS · Card  (Figma node 348:68)
// Source: crates/apollia-desktop/ui/src/lib/components/ui/card/Card.svelte
// Genere depuis figma/manifest.json. Ne pas editer a la main : reexecuter le
// generateur apres toute reconstruction du composant dans Figma.
// Apollia UI is Svelte; this uses the @figma/code-connect HTML parser as the
// publish-ready Svelte stand-in. See ../README.md for plan/status.
import figma, { html } from "@figma/code-connect/html"

figma.connect("<DS_FILE>?node-id=348-68", {
  props: {
    surface: figma.enum("Surface", { "glass": "glass", "solid": "solid" }),
    interactive: figma.enum("Interactive", { "false": "false", "true": "true" }),
    premium: figma.enum("Premium", { "false": "false", "true": "true" }),
  },
  example: (props) => html`<Card surface=${props.surface} interactive=${props.interactive} premium=${props.premium}></Card>`,
})
