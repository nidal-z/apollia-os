# crates/apollia-desktop/ui/AGENTS.md

> Rules for any change under `crates/apollia-desktop/ui/`. This is the only
> frontend rulebook: `docs/agents/FRONTEND-PATTERNS.md` was merged into it and
> deleted (`20bcb771`), because seven of its twelve sections said the same
> thing twice and the two copies had already drifted apart from each other.

Stack : Tauri v2 + Svelte 5 + TypeScript strict + Tailwind 3.4 + lucide-svelte
+ bits-ui + svelte-i18n v4.

Authoritative design reference : `crates/apollia-desktop/ui/src/app.css`
(HSL custom properties, component layers). This file does not duplicate
that catalogue. It encodes the rules to follow when consuming or extending it.

**No directory tree here, and that is a rule rather than an omission.** The one
this file used to carry was wrong on almost every line: a `styles/` directory
that does not exist, a `tokens.css` that never existed, SvelteKit's
`+layout.svelte`, an `i18n/{fr,en}/` tree that is really two flat files, and a
route deleted long ago still described as legacy-but-present. A layout drifts
the moment someone moves a file, and nothing fails when it does. Read the tree
with `ls`; it is never out of date.

---

## 1. TypeScript

**`strict: true` is non-negotiable.** Plus `noUnusedLocals`,
`noFallthroughCasesInSwitch`, `noImplicitOverride`. See
`crates/apollia-desktop/ui/tsconfig.json`.

- Type every component prop. `let { foo, bar = 0 }: { foo: string; bar?: number }
  = $props();` is the canonical form.
- Never `any`. Use `unknown` when the type is genuinely unknown, then narrow.
- Type IPC return values explicitly. Tauri commands return `Promise<unknown>`;
  cast through a typed wrapper, not at the call site.

---

## 2. Svelte 5 runes

New code uses runes only. Svelte 4 reactive statements (`$:`) are forbidden
in new components.

| Rune | Use |
|---|---|
| `$state` | reactive component state |
| `$derived` | computed value |
| `$derived.by` | computed value with a non-trivial expression |
| `$effect` | side effect tied to reactivity |
| `$effect.pre` | side effect before the DOM update |
| `$props` | typed props destructuring |
| `$bindable` | two-way binding contract |

```svelte
<script lang="ts">
  let { count = 0 }: { count?: number } = $props();
  let local = $state(count);
  let doubled = $derived(local * 2);

  $effect(() => {
    console.debug("count.changed", local);
  });
</script>
```

Rules :

- `$effect` runs in the browser after mount. Use `$effect.pre` only when DOM
  measurement before paint matters.
- Cleanup : return a function from `$effect`.
- Never put async work directly inside `$effect`. Spawn it, store the
  abort handle, abort on cleanup.

---

## 3. Design tokens

**Never hardcoded colors, spacings, radii, or shadows.** Always use the HSL
custom properties defined in `crates/apollia-desktop/ui/src/app.css`. Tailwind
reads them via the `tailwind.config.ts` mapping, and
`crates/apollia-desktop/ui/src/lib/design/tokens.ts` exposes them as typed
references so a rename surfaces at type-check time.

```svelte
<!-- WRONG -->
<div style="background: #faf6ec; border: 1px solid #d1cbc0;">

<!-- RIGHT -->
<div class="bg-card border border-border">

<!-- RIGHT (when Tailwind class is not available) -->
<div style="background: hsl(var(--surface-1)); border-color: hsl(var(--border));">
```

Why HSL custom properties : light and dark themes resolve from the same
class name. Hardcoding RGB or hex breaks dark mode silently.

Silently is the point : nothing fails, the component just looks wrong in one
theme, and only in that theme. Find the offenders by pattern rather than by
eye, then check the survivors by toggling `.dark` on `<html>` in DevTools :

```sh
grep -rnE '(bg|text|border)-(neutral|white|black)|#[0-9a-fA-F]{3,8}' \
  crates/apollia-desktop/ui/src --include='*.svelte'
```

A hit is not automatically a defect (an opaque overlay may legitimately be
black), but every hit needs a reason.

Categories of tokens (see `crates/apollia-desktop/ui/src/app.css` for the full table) :

- **Color** : `--primary`, `--surface-1`, `--surface-2`, `--surface-3`,
  `--card`, `--muted`, `--border`, `--destructive`, `--success`, `--warning`,
  `--info`.
- **Text** : `--foreground`, `--text-muted`, `--text-success`, `--text-warning`,
  `--text-danger` (A11y-verified contrasts).
- **Elevation** : elevation tokens with rim lights for dark warmth.
- **Gradients** : `--gradient-primary`, `--gradient-surface`,
  `--gradient-accent`.
- **Glass** : `--glass-border-light`, `--glass-border-dark`, with hover
  variants.

Adding a new token requires : an entry in `crates/apollia-desktop/ui/src/app.css`
(both the light and the dark value), an entry in `tailwind.config.ts` if a class
is needed, and a typed export in
`crates/apollia-desktop/ui/src/lib/design/tokens.ts`.

### The one token with no dark value

`--logo-paper` has a single value, and `.dark` must not override it. This is a
deliberate deviation from the rule above, recorded here rather than left to be
rediscovered.

`/logo.svg` is a light-background artwork. Its dominant swoosh runs from
`#0053da` to `#001eb4`, which measures 1.36:1 against `--card` and 1.57:1
against `--background` in the dark theme, both below the 3:1 floor WCAG 1.4.11
sets for a non-text graphic. On a dark surface the body of the mark does not
dim, it disappears, and only the violet crescent survives. No dark variant of
the vector exists, and producing one means choosing new brand colours, which is
a designer decision and not a frontend one.

So the mark is seated on the paper it was drawn against, identically in both
themes, through the `.logo-plaque` class. Every ink then clears 4.2:1. A themed
plaque would put the mark back on a dark surface in the dark theme, which is the
exact failure it exists to remove. If a dark artwork ever ships, `.logo-plaque`
is the single place to retire.

---

## 3b. The symbol is the product, the spark is not

`Sparkles` from `lucide-svelte` is the generic "AI / suggestion / agent"
affordance and it is used in 33 places. `/logo.svg` is the Apollia mark. They
are not interchangeable, in either direction :

- A spark that stands for **Apollia the product** is wrong. Use `/logo.svg`.
- A logo used to mean **an agent**, **a suggestion**, or **anything generated**
  is equally wrong, and it is the more tempting mistake, because a sweep for
  "replace the spark with the logo" produces it mechanically. An agent avatar
  tile is not the product. A "smart suggestion" hint is not the product.

The surfaces that carry the mark today are the sidebar, the onboarding welcome,
the companion header and the About hero. Everything else that reads as generative
keeps its spark.

Nothing checks this. The distinction is semantic and no grep decides it, so it
is a review item, not a gate. Stated plainly so the next sweep does not turn a
legible 9px glyph into an illegible 9px logo.

---

## 3c. Module size, 800 lines, the same rule as Rust

**Never a `.svelte` or a `.ts` module over 800 lines.** This is the rule
`docs/agents/FORBIDDEN.md` states in its Rust section, and it applies here
unchanged. It is written down in this file because it was not: the rule lived in
the Rust half of the corpus alone, nothing measured the frontend, and six modules
grew past it, the largest at 2358 lines, close to three times the threshold.

What is counted is every line of the file, markup, script and style together. A
`.svelte` file is all three at once and no part of it is free to a reader. Test
modules (`*.test.ts`) and type declarations (`*.d.ts`) are out of scope, as they
are on the Rust side.

Stylesheets are out of scope too, and a long one is the sanctioned way out of a
long component: a co-located `.css` file whose every rule is namespaced under the
component's root class, imported from the script the way
`src/lib/components/ui/markdown/markdown-prose.css` is. Scoped `<style>` cannot
be shared between two components split out of one, and duplicating the rules to
keep the scoping costs more than it buys.

The three ways out, in the order to try them:

1. **A subcomponent**, when a region of the template has its own boundary. It
   takes its markup and its styles with it.
2. **A rune module** (`*.svelte.ts`), when a feature has its own live state.
   `src/components/agents/useAgentActions.svelte.ts` is the shape: a
   `createX()` factory returning an object of `readonly` getters and methods.
3. **A plain module** (`*.ts`), when the code is a decision rather than a state.
   That is the cheapest to test, since Vitest runs in `node` here (section 11).

The rule is measured by `scripts/check_module_size.py`, which reads both sides of
the tree, runs in `just guards`, at pre-commit and in CI, and carries a named
table of exemptions that only ever shrinks. The frontend half of that table is
empty and there is no reason for it to grow.

---

## 4. Tailwind usage

- Utility-first by default. Compose classes inline.
- Extract to a component when the class string passes ~6 utilities AND is
  reused in 2+ places.
- Order : layout > box model > typography > color > effects. (Prettier
  plugin enforces this.)
- Never `@apply` to bundle utilities into a custom class; use a Svelte
  component instead.
- Variants : `dark:`, `hover:`, `focus-visible:`, `disabled:`. Use
  `focus-visible:` not `focus:` for keyboard accessibility.

---

## 5. Components from `bits-ui`

`bits-ui` provides accessible primitives (Dialog, Popover, Select,
DropdownMenu, ...). Use them as the foundation, then style with Tailwind
and tokens.

- Never build a custom modal/popover from scratch. Use the `bits-ui`
  primitive.
- Icons : `lucide-svelte` exclusively. No emoji icons. No custom SVG unless
  the icon is not in lucide.

---

## 6. Tauri IPC

The desktop UI calls into the Rust backend through Tauri commands. Do not
memorise their number, read it from the tree, and read the right one.
`grep -rc '#\[tauri::command\]' crates/apollia-desktop/src`, summed over its
per-file lines, counts the definitions. What the application actually exposes is
the `tauri::generate_handler![` list in `crates/apollia-desktop/src/main.rs`.
The two figures differ whenever a command is `#[cfg]`-gated per target: the
attribute then appears once per target, and the registration once in total.
Patterns here :

```ts
import { invoke } from "@tauri-apps/api/core";

// Typed wrapper, defined once per domain in src/lib/ipc/<domain>.ts
export async function listAgents(): Promise<AgentSummary[]> {
  return invoke<AgentSummary[]>("list_agents");
}
```

Rules :

- Never call `invoke()` directly from a `.svelte` file. Always go through a
  typed wrapper in `src/lib/ipc/`.
- Wrapper functions match the Rust command name in `snake_case`. They are
  exported in `camelCase` for JS consumers.
- Error handling : Tauri commands return `Promise<T>` and reject with the
  serialized Rust error. Wrap in `try/catch` at the call site and surface
  via the toast system (`$lib/toast`).
- Events : `import { listen } from "@tauri-apps/api/event";`. Unsubscribe on
  `$effect` cleanup.

---

## 7. Routing

**This is not SvelteKit.** There is no filesystem router, no `+page.svelte`, no
`+layout.svelte`, and no `$app/navigation`. Writing any of those produces a file
nothing ever mounts.

Routing is a store. `src/lib/stores/navigation.ts` holds a `Route` union type and
a `currentRoute` writable; `src/lib/components/app/Main.svelte` switches on it to
mount the matching component from `src/routes/`.

- Adding a screen means : a component in `src/routes/`, a member in the `Route`
  union, and an arm in the switch. Three edits, no convention magic.
- Navigate with the helpers exported by the navigation store, which also
  maintain back and forward history. Never `window.location`.

---

## 8. Internationalization

`svelte-i18n` v4 with two flat catalogues, `src/lib/i18n/fr.json` and
`src/lib/i18n/en.json`. There is no per-namespace file: the namespace is a key
prefix inside those two files.

```svelte
<script lang="ts">
  import { t } from "svelte-i18n";
</script>

<h1>{$t("transcriptions.title")}</h1>
<p>{$t("transcriptions.intro", { values: { count: items.length } })}</p>
```

Rules :

- Never hardcode user-facing strings. Always go through `$t(...)`.
- Every key has both FR and EN entries. CI fails on parity break.
- Namespace by key prefix : `transcriptions.*`, `agents.*`, `settings.*`.
- Pluralization via ICU MessageFormat where Svelte-i18n supports it,
  otherwise compose at the call site.

---

## 9. State management

- **Local UI state** : `$state` in the component.
- **Cross-component state** : a store in `src/lib/stores/<name>.ts` exposing
  `subscribe` + typed setters. Prefer `writable` from `svelte/store` over
  custom abstractions.
- **Server state** : do not cache Tauri call results in a global store
  unless the data is genuinely shared. Most calls happen on mount of a
  specific screen and stay scoped there.

---

## 10. Operator vs Builder modes

Apollia has two distinct UX modes : **Operator** (autonomy, ease of use,
guided flows) and **Builder** (exhaustive observability, every event, every
field exposed). When fusing duplicated screens or features :

- Preserve the wording and structure of each mode. Do not collapse Operator
  flows into Builder views or vice versa.
- The mode selector is a top-level switch, never per-screen.

---

## 11. Testing

- **Unit : Vitest**, `npm test` from `crates/apollia-desktop/ui/`.
  `vitest.config.ts` sets `environment: "node"` and
  `include: ["src/**/*.test.ts"]`, and `package.json` carries neither a DOM
  environment nor a rendering library, so a test that mounts a component
  cannot run here. Test a component by exporting the logic under test from
  its `<script module>` block and asserting on that export, the way
  `src/components/observability/TaskTimeline.test.ts` does. Anything that
  needs a rendered tree is a Playwright test, below.
- **Browser tests : Playwright**, in `crates/apollia-desktop/ui/tests/`, run
  against the production bundle served by `vite preview` with the Tauri bridge
  stubbed through `window.__TAURI_INTERNALS__.invoke`. They cover UI machinery
  that needs a real browser (dirty state, nav guards, hotkey capture,
  responsive layout). They do **not** exercise the packaged application. Run
  them with `npx playwright test`; the package manager is `npm`, not pnpm.
  **Read this before adding to the corpus, or before believing it.** Measured
  on 2026-08-27, `npx playwright test` returned 19 failures out of 19, every
  one of them at mount rather than on an assertion of substance. Nothing in CI
  runs the corpus, so that red has never been reported anywhere. Two causes,
  both still live for the five remaining specs:
  - The stub is stale. Defining `__TAURI_INTERNALS__` with `invoke` alone is
    worse than defining nothing: the application takes the Tauri path, the
    event API asks for `transformCallback`, and the boot freezes on
    `app-loading`. With no stub at all the same bundle boots to the home page.
  - Four specs navigate through `?route=<page>&sub=<subpage>`, and one through
    `#settings`. No commit of `src/` has ever read those. The application
    routes through the navigation store; only `#design`, `#motion`,
    `#design-empty-states` and `#design-dark-mode` are honoured from the URL,
    and only in dev builds.
  `scripts/check_playwright_specs.py` holds the unreachable-navigation count on
  a descending ratchet and refuses a storage key the UI never reads, which is
  what a spec used to be able to invent without anything noticing. It still
  does not measure whether a spec passes; only running the corpus does that.
- **End-to-end on the packaged application : none in this repository.** macOS
  has no WebDriver for WKWebView, so the Tauri shell cannot be driven by a
  standard browser harness. The runtime paths behind the UI are covered through
  `tests/cli/cli-e2e.sh`, which exercises the same commands against a seeded
  throwaway `HOME`.
- **The real app, driven by hand-written scripts : `scripts/automation/`.** In
  the absence of a WebDriver, a dev-only harness injects a declarative JSON
  script that acts on the DOM through `data-testid` selectors, against the real
  app and a real backend. It is gated behind `debug_assertions` /
  `import.meta.env.DEV` and tree-shaken out of release builds, so it is not a
  packaged end-to-end suite; it is the only thing in the tree that drives the
  shell. Run it with `just desktop-dev-automation-seeded
  scripts/automation/master-det.json` (deterministic, no model) and read the
  verdict from `.apollia-automation/report.json`. Read
  `scripts/automation/README.md` before touching a script or a `data-testid`
  a script names.
- There is no `tauri-driver` setup and no `tests/visual/` baseline suite. Do not
  write a test that assumes either.
- Keep `data-testid` on any surface a test drives. They are the only stable
  handle the UI offers, they cost nothing at runtime, and a renamed one is a
  silently skipped assertion rather than a failure.

---

## 12. When the rules block you

- New token : add to `crates/apollia-desktop/ui/src/app.css` (both modes), to
  `tailwind.config.ts` if a Tailwind class is needed, and to
  `crates/apollia-desktop/ui/src/lib/design/tokens.ts` so a rename surfaces at
  type-check time. Do not reach for a hex value as a shortcut.
- New Tauri command : add the Rust side first
  (`crates/apollia-desktop/src/commands/<domain>.rs`), then the typed
  wrapper, then the consumer. Three commits, one PR, ordered.
- Cross-cutting visual change : open a designer brief
  (structure and wording only, no CSS values) and align before coding.

---

## 13. Panels are one family

"Panel" here means sidebars, sheets, drawers, floating panels, popovers and
overlays together. They get one set of consistency rules rather than one per
shape, because a user does not experience them as different things:

- Motion comes from the design-system tokens, not from ad-hoc durations.
- Stacking comes from the z-index tokens. The set is `--z-backdrop`,
  `--z-overlay`, `--z-toast`, `--z-tooltip`. There is no `--z-modal`, whatever
  older notes claimed; a modal sits on `--z-overlay` above `--z-backdrop`.
- Focus is trapped inside a modal panel, via the `bits-ui` primitive.

Never build a modal or a popover from scratch. `bits-ui` has the accessible
primitive, and hand-rolled ones lose focus handling first and keyboard dismissal
second.
