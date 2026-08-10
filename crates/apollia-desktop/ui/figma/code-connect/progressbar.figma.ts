// Code Connect · Apollia OS · ProgressBar  (Figma node 358:320)
// Source: crates/apollia-desktop/ui/src/lib/components/ui/progress/ProgressBar.svelte
// Genere depuis figma/manifest.json. Ne pas editer a la main : reexecuter le
// generateur apres toute reconstruction du composant dans Figma.
// Apollia UI is Svelte; this uses the @figma/code-connect HTML parser as the
// publish-ready Svelte stand-in. See ../README.md for plan/status.
import figma, { html } from "@figma/code-connect/html"

figma.connect("<DS_FILE>?node-id=358-320", {
  props: {
    size: figma.enum("Size", { "sm": "sm", "md": "md" }),
    variant: figma.enum("Variant", { "primary": "primary", "success": "success", "warning": "warning", "destructive": "destructive", "info": "info" }),
    mode: figma.enum("Mode", { "determinate": "determinate", "indeterminate": "indeterminate" }),
  },
  example: (props) => html`<ProgressBar size=${props.size} variant=${props.variant} mode=${props.mode}></ProgressBar>`,
})
