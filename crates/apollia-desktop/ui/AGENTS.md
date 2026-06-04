# crates/apollia-desktop/ui/AGENTS.md

> Local rules for the Tauri + Svelte desktop UI. Read after
> `docs/agents/INDEX.md` and `docs/agents/FRONTEND-PATTERNS.md` before
> editing this subtree.

Stack : Tauri v2 + Svelte 5 + TypeScript strict + Tailwind 3.4 +
lucide-svelte + bits-ui + svelte-i18n v4. ~114 Tauri IPC commands.

Authoritative design reference : `docs/wiki/DESIGN-SYSTEM.md`. The
front-end never invents tokens or layout primitives; it consumes that
reference.

---

## 1. Directory layout

```
src/
├── App.svelte              # root
├── app.html                # HTML shell
├── styles/
│   ├── app.css             # HSL custom properties (light + dark)
│   └── tokens.css          # design tokens (ADR-021)
├── lib/
│   ├── ipc/                # typed Tauri command wrappers, one file per domain
│   ├── stores/             # svelte/store-based shared state
│   ├── components/         # reusable Svelte components
│   ├── toast/              # toast notification system
│   └── utils/              # helpers
├── routes/
│   ├── +layout.svelte
│   ├── Connections.svelte  # OAuth + MCP install (canonical v0.1.0)
│   ├── Integrations.svelte # legacy, NOT used in v0.1.0
│   ├── Transcriptions.svelte
│   ├── settings/
│   │   ├── Llm.svelte
│   │   ├── Permissions.svelte
│   │   └── Profile.svelte
│   └── ...
├── i18n/
│   ├── fr/                 # French namespaces
│   └── en/                 # English namespaces
└── main.ts
```

`Integrations.svelte` is **legacy**, not used in v0.1.0. Never document it
as canonical. The replacement is `Connections.svelte`.

---

## 2. Component patterns

### Props (typed, destructured)

```svelte
<script lang="ts">
  type Props = {
    label: string;
    onSelect?: (value: string) => void;
    disabled?: boolean;
  };
  let { label, onSelect, disabled = false }: Props = $props();
</script>
```

### Reactive state

```svelte
<script lang="ts">
  let count = $state(0);
  let doubled = $derived(count * 2);

  $effect(() => {
    document.title = `Count: ${count}`;
  });
</script>
```

### Component file naming

`PascalCase.svelte` for components, `+layout.svelte` / `+page.svelte` for
routes (SvelteKit convention).

### Component size

Aim for under 250 lines per `.svelte` file. Past that, extract sub-
components or move logic to a store.

---

## 3. Tauri IPC

The Rust side defines commands in `crates/apollia-desktop/src/commands/`.
The TS side wraps each command in `src/lib/ipc/<domain>.ts`.

```ts
// src/lib/ipc/agents.ts
import { invoke } from "@tauri-apps/api/core";
import type { AgentSummary } from "./types";

export async function listAgents(): Promise<AgentSummary[]> {
  return invoke<AgentSummary[]>("list_agents");
}

export async function startAgent(id: string): Promise<void> {
  return invoke<void>("start_agent", { id });
}
```

Rules :
- Never `invoke()` directly from a `.svelte` file. Always go through a
  typed wrapper in `lib/ipc/`.
- Wrapper exports are `camelCase` ; Rust command names are `snake_case`.
- Error handling at the call site : `try/catch` and surface via the
  toast system.
- Events from the runtime : `listen("event-name", handler)`.
  Unsubscribe in `$effect` cleanup.

Catalogue of ~114 commands : grouped by domain. The full list lives in
`docs/wiki/Reference-Tauri-Commands.md` (post-L2b). Until then, the
authoritative source is the `#[tauri::command]` attributes in
`crates/apollia-desktop/src/commands/`.

---

## 4. Design tokens

All colors, spacings, radii, shadows come from CSS custom properties in
`styles/app.css` (light) and `.dark` overrides. Tailwind reads them via
`tailwind.config.js`.

```svelte
<!-- WRONG -->
<div style="background: #faf6ec; color: #1f2029;">

<!-- RIGHT -->
<div class="bg-card text-card-foreground">
```

ADR-021 defines the v2 token set : elevation, warmth dark, rim lights.
Read it before touching `styles/`.

Adding a new token :
1. Add to `app.css` in **both** light and dark blocks.
2. Add to `tailwind.config.js` if a class shortcut is needed.
3. Document in `docs/wiki/DESIGN-SYSTEM.md`.

---

## 5. i18n

`svelte-i18n` v4 with FR + EN parallel namespaces in
`src/i18n/{fr,en}/<namespace>.json`.

```svelte
<script lang="ts">
  import { t } from "svelte-i18n";
</script>

<h1>{$t("connections.title")}</h1>
```

Rules :
- Never hardcode user-facing strings. Always `$t(...)`.
- Every key has both FR and EN entries. A parity break fails CI.
- Namespace per feature : `connections`, `agents`, `transcriptions`,
  `settings`, `permissions`, ...
- Plural and interpolation via ICU MessageFormat or composed at the
  call site.
- The `lang` attribute on `<html>` is set from the user preference store
  in `+layout.svelte`.

---

## 6. Operator vs Builder modes

Apollia exposes two distinct UX modes :
- **Operator** : autonomy, ease of use, guided flows.
- **Builder** : exhaustive observability, every event, every field
  exposed.

The mode selector is a top-level toggle, not per-screen.

When fusing duplicated screens, preserve the wording and structure of
each mode. Do not collapse Operator into Builder or vice versa.

---

## 7. Sidebars, sheets, drawers, popovers, overlays

"Panels" in Apollia means the full family : sidebars, sheets, drawers,
floating panels, popovers, overlays. Treat them as one category for
consistency rules :

- Animations match the design-system motion tokens.
- Z-index from the tokens (`--z-overlay`, `--z-modal`, ...).
- Focus trap inside modal panels via `bits-ui` primitives.

Never build a custom modal from scratch. Use `bits-ui`.

---

## 8. State management

| Scope | Tool |
|---|---|
| Component-local | `$state` |
| Cross-component, app-wide | store in `lib/stores/<name>.ts` |
| Persisted (across runs) | store + Tauri IPC to `~/.apollia/ui-state.json` |

Stores expose `subscribe` plus typed setters. Avoid custom abstractions
over `svelte/store`.

---

## 9. Forbidden in this subtree

- Svelte 4 reactive declarations (`$:`) in new code.
- Direct `invoke()` from a `.svelte` file.
- Hardcoded colors, spacings, radii. Always tokens.
- Hardcoded user-facing strings (always `$t`).
- `any` in TypeScript. Use `unknown` and narrow.
- Custom modals or popovers (use `bits-ui`).
- Documenting `Integrations.svelte` as canonical (it is legacy).
- CSS values appearing in designer briefs (designer knows the charter).

---

## 10. Testing

- Unit / component : Vitest + `@testing-library/svelte`, co-located as
  `*.test.ts`.
- E2E : Playwright against a built Tauri app.
- Visual regression : Playwright screenshots, baselines committed.

A component test must not call Tauri IPC. Mock the wrapper.

---

## 11. When the rules block you

- New design token : `app.css` (both modes), `tailwind.config.js`,
  `wiki/DESIGN-SYSTEM.md`. Never reach for a hex value as a shortcut.
- New Tauri command : Rust side first, then TS wrapper, then consumer.
  Three commits, one PR, ordered.
- Cross-cutting visual change : open a designer brief (structure and
  wording only, no CSS values) and align before coding.
- Mode-specific exception (Operator does X, Builder does Y) : document
  in the component header comment, never silently diverge.
