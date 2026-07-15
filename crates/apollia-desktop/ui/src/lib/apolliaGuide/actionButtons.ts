/**
 * Apollia Guide - action button helpers.
 *
 * The `apollia-guide` agent ends a reply with an ```apollia-actions``` fenced
 * block: a small JSON array of navigate/invoke buttons. The panel parses that
 * block out of the assistant text (`parseApolliaActions`), validates each
 * button against a strict allowlist (`sanitizeActionButtons`), and renders the
 * survivors inline. To keep the attack surface minimal and prevent
 * prompt-injected links, both the action kind AND the payload shape are
 * validated before a button is rendered.
 *
 * The route allowlist mirrors `agent.py::_ALLOWED_ROUTES`. Parsing and
 * execution are now client-side; the agent is the source of the block.
 */
import { navigateTo, type Route } from "$lib/stores/navigation";

/** Kind of UI action the coach may propose. */
export type CoachActionKind = "navigate" | "invoke";

/**
 * Raw action button as received from the backend. Shape mirrors the Rust
 * `ActionButton` struct - `payload` is untyped on purpose because it
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
  /** Resolved, allowlisted target - `Route` for navigate, command name for invoke. */
  target: string;
  /** Extra query string for navigate actions (e.g. `wizard=open`). `""` when absent. */
  query: string;
}

/**
 * Fenced action block the agent appends to its reply, e.g.
 * ```apollia-actions
 * [{"label": "...", "action": "navigate", "payload": {"route": "/agents"}}]
 * ```
 * Mirrors `agent.py::_ACTION_RE`. `[\s\S]` spans newlines (JS has no DOTALL).
 */
const ACTION_BLOCK_RE = /```apollia-actions\s*(\[[\s\S]*?\])\s*```/;

/**
 * Split a raw assistant reply into its visible text and the raw action
 * buttons carried in its ```apollia-actions``` block. When no block is present
 * (or its JSON is malformed) the text is returned untouched with no buttons.
 * The buttons are still untrusted here - pass them through
 * {@link sanitizeActionButtons} before rendering.
 */
export function parseApolliaActions(content: string): {
  text: string;
  buttons: RawActionButton[];
} {
  if (typeof content !== "string" || content === "") {
    return { text: content ?? "", buttons: [] };
  }
  const match = content.match(ACTION_BLOCK_RE);
  if (!match) {
    return { text: content, buttons: [] };
  }
  let buttons: RawActionButton[] = [];
  try {
    const parsed: unknown = JSON.parse(match[1]);
    if (Array.isArray(parsed)) {
      buttons = parsed as RawActionButton[];
    }
  } catch {
    buttons = [];
  }
  const text = content.replace(ACTION_BLOCK_RE, "").trim();
  return { text, buttons };
}

/**
 * Allowlist of routes the coach is allowed to deep-link to. Mirrors the
 * `_ALLOWED_ROUTES` set in `agent.py` - routes absent from this list are
 * dropped silently.
 */
const ROUTE_ALLOWLIST: Record<string, Route> = {
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
  "/triggers": "automations",
  "/memory": "memory",
  "/observability": "observability",
  "/notifications": "notifications",
  "/settings": "settings",
};

/**
 * Allowlist of Tauri commands the coach may invoke. Currently empty - the
 * `invoke` action kind is reserved for future capabilities (e.g. "Run the
 * onboarding tour from here"). Commands must be explicitly added here AND
 * must be read-only / idempotent.
 */
const INVOKE_ALLOWLIST = new Set<string>([]);

/**
 * Parse a raw action button. Returns `null` if the button fails validation
 * - the caller should skip rendering it.
 */
export function validateActionButton(raw: RawActionButton): SafeActionButton | null {
  if (!raw || typeof raw.label !== "string" || !raw.label.trim()) return null;

  if (raw.action === "navigate") {
    const route = typeof raw.payload?.route === "string" ? raw.payload.route : "";
    const [basePath, query = ""] = route.split("?", 2);
    const resolved = ROUTE_ALLOWLIST[basePath];
    if (!resolved) return null;
    return {
      label: raw.label,
      action: "navigate",
      target: resolved,
      query,
    };
  }

  if (raw.action === "invoke") {
    const command = typeof raw.payload?.command === "string" ? raw.payload.command : "";
    if (!INVOKE_ALLOWLIST.has(command)) return null;
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
 * For `invoke`, reserved for future use - currently a no-op warning.
 */
export async function executeActionButton(btn: SafeActionButton): Promise<void> {
  if (btn.action === "navigate") {
    if (btn.query && globalThis.window !== undefined) {
      const url = new URL(globalThis.location.href);
      url.search = `?${btn.query}`;
      globalThis.history.replaceState({}, "", url.toString());
    }
    navigateTo(btn.target as Route);
    return;
  }

  console.warn("[apolliaGuide] invoke actions are reserved for future use", btn);
}
