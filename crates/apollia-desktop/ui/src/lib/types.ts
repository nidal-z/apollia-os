// Every shared type of the desktop UI, grouped by domain under `types/`.
//
// This file was one 1722-line module. The domains never referenced each
// other enough to justify it, and the 800-line rule of
// `crates/apollia-desktop/ui/AGENTS.md` applies here as it does to Rust.
// Importers keep writing `$lib/types`; nothing about the call sites changed.

export type * from "./types/packages";
export type * from "./types/agents";
export type * from "./types/tasks";
export type * from "./types/llm";
export type * from "./types/triggers";
export type * from "./types/memory";
export type * from "./types/notifications";
export type * from "./types/session";
export type * from "./types/chat";
export type * from "./types/agentic";
export type * from "./types/profile";
export type * from "./types/onboarding";
export type * from "./types/stt";
export type * from "./types/permissions";
export type * from "./types/mcp";
export type * from "./types/projects";
export type * from "./types/reasoning";
