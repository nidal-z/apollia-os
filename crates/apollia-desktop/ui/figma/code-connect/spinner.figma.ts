// Code Connect · Apollia OS · Spinner  (Figma node 358:338)
// Source: crates/apollia-desktop/ui/src/lib/components/ui/progress/Spinner.svelte
// Genere depuis figma/manifest.json. Ne pas editer a la main : reexecuter le
// generateur apres toute reconstruction du composant dans Figma.
// Apollia UI is Svelte; this uses the @figma/code-connect HTML parser as the
// publish-ready Svelte stand-in. See ../README.md for plan/status.
import figma, { html } from "@figma/code-connect/html"

figma.connect("<DS_FILE>?node-id=358-338", {
  props: {
    variant: figma.enum("Variant", { "inline": "inline", "centered": "centered" }),
  },
  example: (props) => html`<Spinner variant=${props.variant}></Spinner>`,
})
