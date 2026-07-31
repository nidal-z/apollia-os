# FRONTEND-PATTERNS

> Rules for any UI change in `crates/apollia-desktop/ui/`. Read this before
> editing Svelte components or Tauri IPC. Pair with
> `crates/apollia-desktop/ui/AGENTS.md` for the local routing map and the
> IPC catalogue.

Stack : Tauri v2 + Svelte 5 + TypeScript strict + Tailwind 3.4 + lucide-svelte
+ bits-ui + svelte-i18n v4.

Authoritative design reference : `crates/apollia-desktop/ui/src/app.css`
(HSL custom properties, component layers, ADR-021). This file does not duplicate
that catalogue. It encodes the rules an LLM must follow when consuming or
extending it.

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
- **Elevation** : ADR-021 elevation tokens with rim lights for dark warmth.
- **Gradients** : `--gradient-primary`, `--gradient-surface`,
  `--gradient-accent`.
- **Glass** : `--glass-border-light`, `--glass-border-dark`, with hover
  variants.

Adding a new token requires : an entry in `crates/apollia-desktop/ui/src/app.css`
(both the light and the dark value), an entry in `tailwind.config.ts` if a class
is needed, and a typed export in
`crates/apollia-desktop/ui/src/lib/design/tokens.ts`.

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

The desktop UI calls into the Rust backend through Tauri commands, 285 of them
at last count. Do not memorise a number: read
`grep -rc '#\[tauri::command\]' crates/apollia-desktop/src`. Patterns here :

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
a `currentRoute` writable; `lib/components/app/Main.svelte` switches on it to
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

- Unit : Vitest. Component tests via `@testing-library/svelte`.
- **Browser tests : Playwright**, in `crates/apollia-desktop/ui/tests/`, run
  against the production bundle served by `vite preview` with the Tauri bridge
  stubbed through `window.__TAURI_INTERNALS__.invoke`. They cover UI machinery
  that needs a real browser (dirty state, nav guards, hotkey capture,
  responsive layout, perf). They do **not** exercise the packaged application.
  Run with `npm run test:perf` and the sibling scripts; the package manager is
  `npm`, not pnpm.
- **End-to-end on the real application : the gestural automaton** in
  `scripts/automation/`. macOS has no WebDriver for WKWebView, so the driver
  injects gestures by `data-testid` into the running Tauri app against a seeded
  throwaway `HOME`. 37 scripts today. Read `scripts/automation/README.md` before
  touching one, and regenerate `master-det` with `tools/regen_master.py` after
  editing a per-page script. Adding a UI surface means adding its `data-testid`s
  and a step in the matching `<page>-det.json`.
- There is no `tauri-driver` setup and no `tests/visual/` baseline suite. Do not
  write a test that assumes either.

---

## 12. When the rules block you

- New token : add to `app.css` (both modes), to `tailwind.config.ts`, to
  `crates/apollia-desktop/ui/src/app.css`. Do not reach for a hex value as a shortcut.
- New Tauri command : add the Rust side first
  (`crates/apollia-desktop/src/commands/<domain>.rs`), then the typed
  wrapper, then the consumer. Three commits, one PR, ordered.
- Cross-cutting visual change : open a designer brief
  (structure and wording only, no CSS values) and align before coding.
