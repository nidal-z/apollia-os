// Code Connect · Apollia OS · DataTable  (Figma node 374:1018)
// Source: crates/apollia-desktop/ui/src/lib/components/ui/data-table/DataTable.svelte
// Genere depuis figma/manifest.json. Ne pas editer a la main : reexecuter le
// generateur apres toute reconstruction du composant dans Figma.
// Apollia UI is Svelte; this uses the @figma/code-connect HTML parser as the
// publish-ready Svelte stand-in. See ../README.md for plan/status.
import figma, { html } from "@figma/code-connect/html"

figma.connect("<DS_FILE>?node-id=374-1018", {
  props: {
    variant: figma.enum("Variant", { "default": "default", "loading": "loading", "empty": "empty" }),
  },
  example: (props) => html`<DataTable variant=${props.variant}></DataTable>`,
})
