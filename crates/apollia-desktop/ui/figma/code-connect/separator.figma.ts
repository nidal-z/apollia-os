// Code Connect · Apollia OS · Separator  (Figma node 358:179)
// Source: crates/apollia-desktop/ui/src/lib/components/ui/separator/Separator.svelte
// Genere depuis figma/manifest.json. Ne pas editer a la main : reexecuter le
// generateur apres toute reconstruction du composant dans Figma.
// Apollia UI is Svelte; this uses the @figma/code-connect HTML parser as the
// publish-ready Svelte stand-in. See ../README.md for plan/status.
import figma, { html } from "@figma/code-connect/html"

figma.connect("<DS_FILE>?node-id=358-179", {
  props: {
    orientation: figma.enum("Orientation", { "horizontal": "horizontal", "vertical": "vertical" }),
    variant: figma.enum("Variant", { "solid": "solid", "subtle": "subtle", "elevated": "elevated", "fade": "fade", "inline": "inline", "dashed": "dashed" }),
    color: figma.enum("Color", { "border": "border", "muted": "muted", "primary": "primary" }),
  },
  example: (props) => html`<Separator orientation=${props.orientation} variant=${props.variant} color=${props.color}></Separator>`,
})
