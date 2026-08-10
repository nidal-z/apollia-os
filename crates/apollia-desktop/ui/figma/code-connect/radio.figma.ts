// Code Connect · Apollia OS · Radio  (Figma node 356:202)
// Source: crates/apollia-desktop/ui/src/lib/components/ui/radio/RadioItem.svelte
// Genere depuis figma/manifest.json. Ne pas editer a la main : reexecuter le
// generateur apres toute reconstruction du composant dans Figma.
// Apollia UI is Svelte; this uses the @figma/code-connect HTML parser as the
// publish-ready Svelte stand-in. See ../README.md for plan/status.
import figma, { html } from "@figma/code-connect/html"

figma.connect("<DS_FILE>?node-id=356-202", {
  props: {
    state: figma.enum("State", { "unchecked": "unchecked", "checked": "checked", "focus": "focus", "disabled-unchecked": "disabled-unchecked", "disabled-checked": "disabled-checked", "loading": "loading" }),
  },
  example: (props) => html`<Radio state=${props.state}></Radio>`,
})
