# ADR-021: Frontend design system and internationalization

- Status: Accepted
- Date: 2026-06-04

## Context

The desktop frontend ([ADR-020](ADR-020-desktop-architecture.md)) needs two
foundations before any new view is built: a coherent visual system and a clean
bilingual layer. Two problems make both decisions urgent.

The visual system was a flat HSL palette plus hardcoded `box-shadow` values
scattered across components. Before the refactor a grep for `box-shadow:` in the
Svelte tree returned dozens of independent hardcoded values (around forty-eight
at the time), so any brand change meant editing each one with no guarantee of
consistency. Surfaces were flat, with a single fixed
shadow and no elevation hierarchy distinguishing a table row from a floating menu
or a hero modal. The dark mode was neutral-cold (a `240` hue) which clashed with
the blue/violet identity, and several glass insets were effectively invisible in
dark. Five key call-to-action buttons used a generic primary background,
indistinguishable from a secondary button.

The string layer was mixed French and English: hardcoded strings in components,
French labels baked into English components, and no convention for icon-only
`aria-label` attributes, so an operator running the interface in French still met
English text and an unreadable keyboard path. The runtime already shipped
`svelte-i18n` with French and English catalogs of roughly seventeen hundred keys,
a language switch, and a parity test, but usage was incomplete and the naming
convention was undocumented.

## Decision

We adopt design tokens v2 in `app.css` as the single source of truth for
elevation, surfaces, and primary exposure, banning static hardcoded `box-shadow`
literals in components, and `svelte-i18n` as the sole internationalization
mechanism with
French as the default locale, English as fallback, and JSON catalogs backed by a
typed per-zone index.

### Design tokens v2

The tokens live in `crates/apollia-desktop/ui/src/app.css` and are the single
source of truth for every shadow, surface, and primary utility. The rule:
static `box-shadow` literals are banned in components, every static shadow goes
through a `var(...)`. The residual exceptions are dynamic, color-driven glows
(for example a status dot or a swatch whose color is bound at runtime) and a
small documented set of premium surfaces. Five concrete pieces:

- Elevation scale. Five levels, `--shadow-elev-0` through `--shadow-elev-4`. Each
  level carries an `inset 0 1px 0` rim-light layer for a top-edge highlight, which
  gives the material feel, over its outer shadow. The base level `--shadow-elev-1`
  combines two outer shadow layers (near and far) for depth, while the other
  levels use a single outer layer plus the inset rim. The inset rim is white at
  varying opacity, which resolves the dark-on-dark invisibility. Each level is
  redeclared in dark with adapted opacities, the top level carrying a
  primary-tinted secondary glow.
- Warm dark surfaces. The dark palette shifts from a neutral `240` hue toward a
  warm-brown hue so it contrasts tangibly with the blue/violet primary. Three
  surface levels (`--surface-1` through `--surface-3`, raised to recessed) add to
  background and muted for depth hierarchies.
- Primary utilities. Three explicit classes, also exposed as Button variants:
  `.bg-primary-solid` (solid fill with a primary shadow on hover),
  `.bg-primary-gradient` (a primary-to-secondary gradient with elevation), and
  `.border-primary-subtle` (a primary border that intensifies on hover). They are
  applied to the five key call-to-action buttons so the visual identity is
  obvious at a glance. Tailwind also exposes primary-tinted shadows.
- Differentiated glass borders. Separate light, light-hover, dark, and dark-hover
  border tokens, with a `.glass-border` class that switches automatically on
  hover, so borders keep contrast in both modes.
- Mode-specific backdrop. A darker plain backdrop with blur in light, a
  warm-tinted backdrop with blur in dark, exposed as `.app-backdrop` and a
  subtler variant for popovers.

A companion TypeScript export of the tokens prepares future consumption by a
theme builder. Visual regression stays largely grep-friendly: a search for
`box-shadow:` returns `var(...)` references for the static cases, the residual
hits being the documented dynamic glows.

### Internationalization with svelte-i18n

`svelte-i18n` is the sole mechanism, with French as default and English as
fallback. The operational rules:

- The JSON catalogs (`en.json`, `fr.json`) are the source of truth. Every new
  string passes through both JSON files first and is never hardcoded in a
  `.svelte` file.
- A typed per-zone index (`strings/*.ts`) exposes typed key constants for keys
  consumed programmatically, giving a grep-friendly map of keys by zone.
- Key convention: `zone.sub_zone.context` in snake_case. Icon-only `aria-label`
  keys always live under `a11y.<name>`.
- Capitalization convention: sentence case in both French and English, aligned on
  the modern Material and Atlassian convention. System badges stay uppercase, and
  brand names and technical identifiers keep their own casing.
- Locale detection at init: a persisted local preference if set, otherwise the
  navigator locale when supported, otherwise French. The user can switch from
  Settings and the choice is persisted locally.
- Brand whitelist: brand names and technical identifiers are not translated, and
  example placeholders are left as is.
- Design-system showcase views are dev-only and excluded from translation.

A CI audit script greps for remaining hardcoded strings and exits non-zero if the
inventory is not empty, and a locale-switch test verifies that flipping the
locale flips the strings.

## Alternatives considered

### Extend native Tailwind shadows (rejected)
- Pros: no new system, stays within Tailwind.
- Cons: Tailwind does not support inset rim lights ergonomically, and the system
  could not be exposed on the TypeScript side without duplication.

### JS-driven tokens behind a theme store (rejected)
- Pros: dynamic from JavaScript.
- Cons: a store implies re-renders and cannot be used inside `@keyframes`. CSS
  variables switch mode through a class with no JavaScript.

### paraglide-js for i18n (rejected)
- Pros: optimal bundle through tree-shaking, strong native typing, full ICU
  message format.
- Cons: roughly seventeen hundred keys already exist on `svelte-i18n` across the
  component tree (around two hundred and eighty `.svelte` files), so migrating is
  net risk with no visible user gain. It stays an option for a future major
  rewrite.

### Hand-rolled JSON plus a getter (rejected)
- Pros: zero runtime dependency, full control.
- Cons: it would re-implement interpolation, ICU pluralization, and the reactive
  Svelte store that `svelte-i18n` already provides.

### Chosen: tokens v2 in app.css plus svelte-i18n
- Pros: immediate visual coherence (the pre-refactor sprawl of hardcoded shadow
  values, around forty-eight at the time, collapses into a handful of reused
  tokens), a measurably warm dark mode, identity-bearing
  call-to-action buttons, a fully French operator interface once the audit is
  green, and a grep-friendly key map.
- Trade-offs: a visual regression on token edits is not covered automatically and
  the showcase route must be checked by hand; the JSON catalog is large but loads
  synchronously, which is acceptable on desktop; and the capitalization
  convention is enforced by review, not mechanically.

## Consequences

- Positive: a single edit to `app.css` propagates a brand change across the app,
  the dark mode is tangibly warm, and a French operator gets a fully French
  interface, icon-only labels included, once the i18n audit passes.
- Negative / trade-off: visual regressions require manual showcase verification,
  the i18n catalog is large, and the capitalization convention relies on code
  review.
- Watch: the event of new locales doubling the catalog (which would justify
  lazy-loading per locale), generalizing ICU pluralization as new counters
  appear, and keeping the showcase route in sync with the tokens.

## Architectural principles

- Principle #1 (Local-first): the locale preference is persisted locally with no
  network call, and the tokens are embedded in the binary.
- Principle #8 (Human CLI, machine API): the UI is human and French by default,
  while test identifiers stay machine-readable and untranslated.

## Related

- [ADR-020](ADR-020-desktop-architecture.md) the desktop architecture whose
  bits-ui primitives this system styles and translates.
