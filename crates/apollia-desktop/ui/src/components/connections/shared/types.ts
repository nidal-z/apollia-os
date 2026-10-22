/**
 * Shared types for the Connections surface, used by the orchestrator route and
 * its sidebar / detail / catalogue sub-components.
 */
import type { ProviderId } from "$lib/connections/capabilities";
import type { RowSelection } from "$lib/connections/connectionRows";

/**
 * Which connection the detail pane shows. A discriminated union so the right
 * pane knows which tabs to render and which actions to wire.
 */
export type Selection = RowSelection | null;

/** The page of a native provider (Google Workspace, Microsoft 365). */
export interface NativeConnectorCard {
  id: ProviderId;
  name: string;
  description: string;
}

/** How many built-in tools are on, out of how many. */
export interface BuiltinToolsCount {
  active: number;
  total: number;
}

/** Custom MCP transport kinds. */
export type CustomTransport = "stdio" | "streamable-http" | "sse";
