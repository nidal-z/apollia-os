// Code Connect · Apollia OS · Accordion  (Figma node 57:26)
// Source: crates/apollia-desktop/ui/src/lib/components/ui/accordion/AccordionItem.svelte
// Apollia UI is Svelte; this uses the @figma/code-connect HTML parser as the
// publish-ready Svelte stand-in. See ../README.md for plan/status.
import figma, { html } from "@figma/code-connect/html"

figma.connect("<DS_FILE>?node-id=57-26", {
  props: {
    state: figma.enum("State", { "collapsed": "collapsed", "expanded": "expanded" }),
  },
  example: (props) => html`<Accordion state=${props.state}></Accordion>`,
})
