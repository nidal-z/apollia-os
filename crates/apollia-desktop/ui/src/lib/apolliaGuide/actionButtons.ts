/**
 * Apollia Guide — action button helpers (US-SP42-057).
 *
 * The meta-chat coach (backend `apollia_coach_invoke`) returns structured
 * `action_buttons` alongside its narrative text. The frontend renders them
 * inline inside the reply bubble. To keep the attack surface minimal and
 * prevent prompt-injected links, we enforce a **strict whitelist** here —
 * both the action kind AND the payload shape are validated before the
 * button is rendered.
 *
 * Mirrored server-side in `agent.py::_ALLOWED_ROUTES` and (loosely) in
 * `apollia_coach.rs::extract_action_block`. Kept in sync manually; the
 * Rust side is authoritative for parsing, this module is authoritative
 * for execution.
 */
import { navigateTo, type Route } from "$lib/stores/navigation";

/** Kind of UI action the coach may propose. */
export type CoachActionKind = "navigate" | "invoke";

/**
 * Raw action button as received from the backend. Shape mirrors the Rust
 * `ActionButton` struct — `payload` is untyped on purpose because it
 * varies per action kind.
 */
export interface RawActionButton {
  label: string;
  action: CoachActionKind;
  payload?: Record<string, unknown> | null;
}

/** Post-validation action button ready to render. */
export interface SafeActionButton {
  label: string;
  action: CoachActionKind;
  /** Resolved, whitelisted target — `Route` for navigate, command name for invoke. */
  target: string;
  /** Extra query string for navigate actions (e.g. `wizard=open`). `""` when absent. */
  query: string;
}

/**
 * Whitelist of routes the coach is allowed to deep-link to. Mirrors the
 * `_ALLOWED_ROUTES` set in `agent.py` — routes absent from this list are
 * dropped silently.
 */
const ROUTE_WHITELIST: Record<string, Route> = {
  "/dashboard": "dashboard",
  "/agents": "agents",
  "/projects": "projects",
  "/tasks": "tasks",
  "/chat": "chat",
  "/automations": "automations",
  "/integrations": "integrations",
  "/inbox": "inbox",
  "/onboarding": "onboarding",
  "/llm": "llm",
  "/triggers": "triggers",
  "/pipelines": "pipelines",
  "/memory": "memory",
  "/observability": "observability",
  "/notifications": "notifications",
  "/settings": "settings",
};

/**
 * Whitelist of Tauri commands the coach may invoke. Currently empty — the
 * `invoke` action kind is reserved for future capabilities (e.g. "Run the
 * onboarding tour from here"). Commands must be explicitly added here AND
 * must be read-only / idempotent.
 */
const INVOKE_WHITELIST = new Set<string>([]);

/**
 * Parse a raw action button. Returns `null` if the button fails validation
 * — the caller should skip rendering it.
 */
export function validateActionButton(raw: RawActionButton): SafeActionButton | null {
  if (!raw || typeof raw.label !== "string" || !raw.label.trim()) return null;

  if (raw.action === "navigate") {
    const route = typeof raw.payload?.route === "string" ? (raw.payload.route as string) : "";
    const [basePath, query = ""] = route.split("?", 2);
    const resolved = ROUTE_WHITELIST[basePath];
    if (!resolved) return null;
    return {
      label: raw.label,
      action: "navigate",
      target: resolved,
      query,
    };
  }

  if (raw.action === "invoke") {
    const command = typeof raw.payload?.command === "string" ? (raw.payload.command as string) : "";
    if (!INVOKE_WHITELIST.has(command)) return null;
    return {
      label: raw.label,
      action: "invoke",
      target: command,
      query: "",
    };
  }

  return null;
}

/**
 * Filter + normalise a backend action button list. Caps at 3 to match the
 * server-side limit and prevent visual clutter even if the LLM emits more.
 */
export function sanitizeActionButtons(raw: RawActionButton[] | null | undefined): SafeActionButton[] {
  if (!Array.isArray(raw)) return [];
  const out: SafeActionButton[] = [];
  for (const btn of raw) {
    const safe = validateActionButton(btn);
    if (safe) out.push(safe);
    if (out.length === 3) break;
  }
  return out;
}

/**
 * Execute a sanitised action button. For `navigate`, delegates to the
 * navigation store and preserves the query string in `window.location.search`
 * so downstream code that reads the query (e.g. the automation wizard's
 * `?wizard=open`) keeps working.
 *
 * For `invoke`, reserved for future use — currently a no-op warning.
 */
export async function executeActionButton(btn: SafeActionButton): Promise<void> {
  if (btn.action === "navigate") {
    if (btn.query && typeof window !== "undefined") {
      const url = new URL(window.location.href);
      url.search = `?${btn.query}`;
      window.history.replaceState({}, "", url.toString());
    }
    navigateTo(btn.target as Route);
    return;
  }

  console.warn("[apolliaGuide] invoke actions are reserved for future use", btn);
}
