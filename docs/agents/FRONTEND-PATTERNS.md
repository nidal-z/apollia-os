# FRONTEND-PATTERNS

> Rules for any UI change in `crates/apollia-desktop/ui/`. Read this before
> editing Svelte components or Tauri IPC. Pair with
> `crates/apollia-desktop/ui/AGENTS.md` for the local routing map and the
> 114-command IPC catalogue.

Stack : Tauri v2 + Svelte 5 + TypeScript strict + Tailwind 3.4 + lucide-svelte
+ bits-ui + svelte-i18n v4.

Authoritative design reference : `docs/wiki/DESIGN-SYSTEM.md` (1160 lines,
HSL tokens, components, ADR-077). This file does not duplicate that catalogue.
It encodes the rules an LLM must follow when consuming or extending it.

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
custom properties defined in `crates/apollia-desktop/ui/src/styles/app.css`
(or `tokens.css` if present). Tailwind reads them via the
`tailwind.config.js` mapping.

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

Categories of tokens (see `wiki/DESIGN-SYSTEM.md` for the full table) :

- **Color** : `--primary`, `--surface-1`, `--surface-2`, `--surface-3`,
  `--card`, `--muted`, `--border`, `--destructive`, `--success`, `--warning`,
  `--info`.
- **Text** : `--foreground`, `--text-muted`, `--text-success`, `--text-warning`,
  `--text-danger` (A11y-verified contrasts).
- **Elevation** : ADR-077 elevation tokens with rim lights for dark warmth.
- **Gradients** : `--gradient-primary`, `--gradient-surface`,
  `--gradient-accent`.
- **Glass** : `--glass-border-light`, `--glass-border-dark`, with hover
  variants.

Adding a new token requires : entry in `app.css` (both light and dark
values), entry in `tailwind.config.js` if a class is needed, line in
`wiki/DESIGN-SYSTEM.md`.

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

The desktop UI calls into the Rust backend via `~114 Tauri commands`. Full
catalogue in `crates/apollia-desktop/ui/AGENTS.md`. Patterns here :

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

SvelteKit-style routes in `src/routes/`. One file per view. Layouts in
`+layout.svelte`. Loaders live in `+page.ts` when present.

- One route = one screen. No multi-purpose pages.
- Navigation : `import { goto } from "$app/navigation"`. Never window.location.

---

## 8. Internationalization

`svelte-i18n` v4 with parallel FR + EN JSON namespaces in
`src/i18n/<lang>/<namespace>.json`.

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
- Namespace per feature : `transcriptions`, `agents`, `settings`, ...
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
- E2E : Playwright against a built Tauri app, or `tauri-driver` for native
  shell integration.
- Visual regression : Playwright screenshots in `tests/visual/`. Update
  baselines via `pnpm test:visual:update`.
- See `docs/agents/TESTING.md` for the cross-stack matrix.

---

## 12. When the rules block you

- New token : add to `app.css` (both modes), to `tailwind.config.js`, to
  `wiki/DESIGN-SYSTEM.md`. Do not reach for a hex value as a shortcut.
- New Tauri command : add the Rust side first
  (`crates/apollia-desktop/src/commands/<domain>.rs`), then the typed
  wrapper, then the consumer. Three commits, one PR, ordered.
- Cross-cutting visual change : open a designer brief
  (structure and wording only, no CSS values) and align before coding.
