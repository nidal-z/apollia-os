/**
 * Shared types for the Connections surface, used by the orchestrator route and
 * its sidebar / detail / catalogue sub-components.
 */
import type { ProviderId } from "$lib/connections/capabilities";

/**
 * Which connector the detail pane shows. A discriminated union so the right
 * pane knows which tabs to render and which actions to wire.
 */
export type Selection =
  | { kind: "native"; provider: ProviderId }
  | { kind: "mcp"; name: string }
  | null;

/** A native Apollia connector card (Google Workspace, Microsoft 365). */
export interface NativeConnectorCard {
  id: ProviderId;
  name: string;
  description: string;
}

/** Status filter values for the sidebar chip bar. */
export type StatusFilter = "all" | "active" | "attention" | "error" | "idle";

/** Context-menu action requested from an MCP sidebar row. */
export type McpRowAction = "test" | "configure" | "disconnect";

/** MCP detail tab keys. */
export type McpTab = "overview" | "tools" | "settings";

/** Native connector detail tab keys. */
export type NativeTab = "accounts" | "capabilities" | "settings";

/** Live-test verdict for an MCP server. */
export type TestState =
  | "idle"
  | "testing"
  | "ok"
  | "unverified"
  | "degraded"
  | "needs_reauth"
  | "error";

/** Tool-approval level for an MCP server. */
export type ApprovalLevel = "auto" | "ask" | "readonly";

/** Custom MCP transport kinds. */
export type CustomTransport = "stdio" | "streamable-http" | "sse";
