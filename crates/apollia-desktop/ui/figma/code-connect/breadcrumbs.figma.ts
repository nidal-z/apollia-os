// Code Connect · Apollia OS · Breadcrumbs  (Figma node 57:15)
// Source: crates/apollia-desktop/ui/src/lib/components/ui/breadcrumbs/Breadcrumbs.svelte
// Apollia UI is Svelte; this uses the @figma/code-connect HTML parser as the
// publish-ready Svelte stand-in. See ../README.md for plan/status.
import figma, { html } from "@figma/code-connect/html"

figma.connect("<DS_FILE>?node-id=57-15", {
  props: {
    variant: figma.enum("Variant", { "default": "default", "collapsed": "collapsed" }),
  },
  example: (props) => html`<Breadcrumbs variant=${props.variant}></Breadcrumbs>`,
})
