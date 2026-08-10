// Code Connect · Apollia OS · Avatar  (Figma node 373:1058)
// Source: crates/apollia-desktop/ui/src/lib/components/ui/avatar/Avatar.svelte
// Genere depuis figma/manifest.json. Ne pas editer a la main : reexecuter le
// generateur apres toute reconstruction du composant dans Figma.
// Apollia UI is Svelte; this uses the @figma/code-connect HTML parser as the
// publish-ready Svelte stand-in. See ../README.md for plan/status.
import figma, { html } from "@figma/code-connect/html"

figma.connect("<DS_FILE>?node-id=373-1058", {
  props: {
    size: figma.enum("Size", { "xs": "xs", "sm": "sm", "md": "md", "lg": "lg", "xl": "xl" }),
    ring: figma.enum("Ring", { "false": "false", "true": "true" }),
  },
  example: (props) => html`<Avatar size=${props.size} ring=${props.ring}></Avatar>`,
})
