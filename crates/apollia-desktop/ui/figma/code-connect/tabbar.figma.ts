// Code Connect · Apollia OS · TabBar  (Figma node 347:48)
// Source: crates/apollia-desktop/ui/src/lib/components/ui/tabs/TabBar.svelte
// Genere depuis figma/manifest.json. Ne pas editer a la main : reexecuter le
// generateur apres toute reconstruction du composant dans Figma.
// Apollia UI is Svelte; this uses the @figma/code-connect HTML parser as the
// publish-ready Svelte stand-in. See ../README.md for plan/status.
import figma, { html } from "@figma/code-connect/html"

figma.connect("<DS_FILE>?node-id=347-48", {
  props: {
    variant: figma.enum("Variant", { "pill": "pill", "underline": "underline" }),
  },
  example: (props) => html`<TabBar variant=${props.variant}></TabBar>`,
})
