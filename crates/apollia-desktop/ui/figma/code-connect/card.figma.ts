// Code Connect · Apollia OS · Card  (Figma node 35:26)
// Source: crates/apollia-desktop/ui/src/lib/components/ui/card/Card.svelte
// Apollia UI is Svelte; this uses the @figma/code-connect HTML parser as the
// publish-ready Svelte stand-in. See ../README.md for plan/status.
import figma, { html } from "@figma/code-connect/html"

figma.connect("<DS_FILE>?node-id=35-26", {
  props: {
    surface: figma.enum("Surface", { "glass": "glass", "solid": "solid" }),
    interactive: figma.boolean("Interactive"),
    premium: figma.boolean("Premium"),
  },
  example: (props) => html`<Card surface=${props.surface} interactive=${props.interactive} premium=${props.premium}></Card>`,
})
