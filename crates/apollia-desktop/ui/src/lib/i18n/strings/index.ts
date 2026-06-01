/**
 * Typed i18n key catalog - one module per zone.
 *
 * The actual translations live in `../en.json` / `../fr.json`. These TS
 * modules are the typed indexes consumed by components that want to pass
 * keys around programmatically (e.g. `EmptyState` variants, table column
 * definitions). Keeping them here gives us:
 *  - a grep target when deciding if a string needs a new key or if one
 *    already exists for the same zone,
 *  - a single place to audit coverage when a new zone appears,
 *  - compile-time errors when a referenced key is renamed in the JSON.
 *
 * See ADR-076 (docs/adr/ADR-076-i18n-frontend.md) for the key naming
 * convention and catalog organisation rationale.
 */
export * from "./empty-states";
export * from "./common";
export * from "./a11y";
export * from "./dashboard";
export * from "./chat";
export * from "./agents";
export * from "./tasks";
export * from "./triggers";
export * from "./settings";
export * from "./integrations";
export * from "./notifications";
export * from "./observability";
export * from "./projects";
export * from "./command";
