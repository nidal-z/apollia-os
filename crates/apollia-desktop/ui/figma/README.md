# Figma Code Connect, Apollia OS Design System

Version-controls the link between this Svelte codebase and the Figma file
**"Apollia OS Design System"** (`2TLZ2uqIOweX14eP4VGXHq`).

- `../figma.config.json` , Code Connect config (HTML parser; `<DS_FILE>` resolved via `documentUrlSubstitutions`).
- `MAPPING.md` , complete node-id <-> source-file manifest (primitives, feature components, route templates, tokens).
- `code-connect/*.figma.ts` , one publish-ready Code Connect definition per primitive (variant props mapped via `figma.enum` / `figma.boolean`; node-ids match `MAPPING.md`).

## Status

> Live Code Connect publish (and in-Figma Dev Mode mapping) requires a Figma
> Organization/Enterprise plan with a Developer seat. This workspace is on a
> Pro plan, so `figma connect publish` is not available yet. Everything here is
> version-controlled and ready to publish the moment the workspace is upgraded.

Apollia UI is **Svelte**, which Code Connect does not parse natively. The
`.figma.ts` files use the `@figma/code-connect/html` parser as the publish-ready
Svelte stand-in; the authoritative, parser-independent mapping lives in
`MAPPING.md`.

## Enabling live Code Connect (after upgrade)

```bash
cd crates/apollia-desktop/ui
npm i -D @figma/code-connect
# the Figma library must be published to a team library first
npx figma connect publish --token <FIGMA_ACCESS_TOKEN>
```

## Keeping code <-> design in sync

- **Tokens:** `src/app.css` (`:root` + `.dark`) and `tailwind.config.ts` are the
  source of truth. If a token changes, update the matching Figma variable
  (Color collection carries Light + Dark modes).
- **Components:** when a component's variant set changes, update its
  `.figma.ts` and the Figma variant set, and bump the node-id in `MAPPING.md`.
- **New components:** build in Figma, record the node-id in `MAPPING.md`, add a
  `.figma.ts`.
- The Figma file's `🔍 Audit` page mirrors `MAPPING.md` and reports coverage.
