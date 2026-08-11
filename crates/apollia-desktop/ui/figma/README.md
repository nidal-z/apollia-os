# Figma link, Apollia OS Design System

Version-controls the link between this Svelte codebase and the Figma file
**"Apollia OS Design System"** (`2TLZ2uqIOweX14eP4VGXHq`).

- `manifest.json` , the authoritative node-id to source-file record, tracked by
  git. One entry per Figma component: `nodeId`, `name`, `page`, `source`,
  `variants`, and the variant axes read from the component's `Props` interface.
  It also records the declared substitutions and what Figma cannot represent.
- `code-connect/*.figma.ts` , publish-ready Code Connect definitions, kept for
  the day the workspace can publish them. **They are not the source of truth**
  and their node ids are not guaranteed current.
- `../figma.config.json` , Code Connect config (HTML parser; `<DS_FILE>`
  resolved via `documentUrlSubstitutions`).

## Why a manifest rather than Code Connect

Live Code Connect publish, and in-Figma Dev Mode mapping, require a Figma
Organization or Enterprise plan with a Developer seat. This workspace is on a
Pro plan, so `figma connect publish` is refused by the API.

Apollia UI is **Svelte**, which Code Connect does not parse natively either.
The link therefore rests on three redundant markers, all mechanically
checkable without any Figma plan:

1. the Figma component name, which is the PascalCase name of the `.svelte`
   file, disambiguated by its folder in parentheses when two trees collide
   (`EmptyState (operator)`, `Keycap (settings)`);
2. the first line of the Figma component description, which is the source path
   relative to `crates/apollia-desktop/ui/src/`;
3. `manifest.json`, which records both plus the live node id.

Checking the link needs no Figma access at all:

```sh
cd "$(git rev-parse --show-toplevel)"
python3 -c "
import json, os
m = json.load(open('crates/apollia-desktop/ui/figma/manifest.json'))
root = 'crates/apollia-desktop/ui/src/'
missing = [c for c in m['components'] if not os.path.exists(root + c['source'])]
print(f'entries: {len(m[\"components\"])}, missing source files: {len(missing)}')
for c in missing: print('  ', c['name'], '->', c['source'])
"
```

## File structure

| Page | Content |
|---|---|
| `🎨 Foundations` | the three variable collections and the twenty-one text styles |
| `🧩 Primitives` | 78 components from `lib/components/ui/` plus the logic-free bricks promoted from `operator/`, `layout/`, `feedback/`, `shared/` |
| `🧱 Composants` | 83 business components, assembled from primitive instances |
| `🔣 Icones` | 60 icons, vectors extracted from the repo's own `lucide-svelte` package |
| `🔍 Couverture` | the coverage matrix |

## Enabling live Code Connect (after an eventual upgrade)

```sh
cd crates/apollia-desktop/ui
npm i -D @figma/code-connect
# the Figma library must be published to a team library first
npx figma connect publish --token <FIGMA_ACCESS_TOKEN>
```

The `.figma.ts` files need their node ids refreshed from `manifest.json` before
that command can succeed.

## Keeping code and design in sync

- **Tokens:** `src/app.css` and `tailwind.config.ts` are the source of truth for
  values. Figma is the source of truth for composition. When a token changes,
  update the matching Figma variable; the `app.css` collection carries the Light
  and Dark modes.
- **Components:** when a variant set changes, read the new set from the `Props`
  interface, update the Figma variant set, and update the entry in
  `manifest.json`.
- **New components:** build in Figma from primitive instances, put the source
  path on the first line of the description, and add the entry to
  `manifest.json`.
- **Verification:** a component is done when it has been rendered and looked at
  in both Light and Dark. A component that does not switch modes correctly
  carries a hardcoded value, which is a construction defect rather than a
  setting.
