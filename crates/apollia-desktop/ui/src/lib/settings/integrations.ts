/**
 * Settings > Integrations: what Apollia grants by default (the accesses of a
 * new agent created without code, the profile in a new conversation, the MCP
 * servers a request may bring) and, read only, what is always authorized.
 *
 * The defaults are named settings of `apollia.toml`, read and written through
 * the shell with the source of their value, so the organization policy can
 * impose each of them. Everything else on the section is a link: the detail
 * lives on the Authorizations and Connections screens.
 */
import {
  getResolvedSetting,
  setSetting,
  type ResolvedSetting,
  type SettingSource,
} from "$lib/ipc/settings";
import { navigateTo } from "$lib/stores/navigation";
import { isSensitiveFamily } from "$lib/tools/action-family";

/** Families a new agent is granted whole (`[tools.new_agent] families`). */
export const NEW_AGENT_FAMILIES_KEY = "tools.new_agent.families";
/** Tools a new agent is granted by name (`[tools.new_agent] access`). */
export const NEW_AGENT_ACCESS_KEY = "tools.new_agent.access";
/** The profile switch a new conversation is born with. */
export const SHARE_PROFILE_DEFAULT_KEY = "chat.share_profile_default";
/** Whether a request to the compatible server may bring an MCP server. */
export const ALLOW_REQUEST_MCP_KEY = "server.allow_request_mcp";

/** The blocks of the section, in order, by the i18n key of their title. */
export const INTEGRATIONS_BLOCKS = [
  "settings.integrations_section.new_agent_title",
  "settings.integrations_section.new_conversation_title",
  "settings.integrations_section.always_title",
  "settings.integrations_section.connections_title",
  "settings.integrations_section.developer_title",
] as const;

/** The blocks shown with the developer mode in this position. */
export function visibleIntegrationsBlocks(developerMode: boolean): readonly string[] {
  return developerMode ? INTEGRATIONS_BLOCKS : INTEGRATIONS_BLOCKS.slice(0, 4);
}

/** The `data-testid` of each actionable element of the section. */
export const INTEGRATIONS_TEST_IDS = {
  section: "integrations-settings-section",
  access: "integrations-new-agent-access",
  accessPolicy: "integrations-new-agent-access-policy",
  shareProfile: "integrations-share-profile",
  always: "integrations-always-summary",
  alwaysEmpty: "integrations-always-empty",
  openAuthorizations: "integrations-open-authorizations",
  openConnections: "integrations-open-connections",
  requestMcp: "integrations-allow-request-mcp",
} as const;

/** The tool a checked code folder grants, never an access of the list. */
const CODE_CONTEXT_TOOL = "code_context";

/** A tool of the machine, as the section needs it. */
export interface DefaultAccessTool {
  name: string;
  /** Action family; `unclassified` when nobody classified the tool. */
  family: string;
}

/** One line of the accesses a new agent is granted. */
export interface DefaultAccessRow {
  id: string;
  /** Granted to every new agent. */
  checked: boolean;
  /** Its family is anything but plain reading. */
  sensitive: boolean;
  /** Granted by a whole family: the line is not unchecked one tool at a time. */
  byFamily: boolean;
}

/**
 * The accesses of the section: every tool of the machine but those that act
 * on the conversation alone and the code context, the ones a family grants
 * first, then the sensitive ones, each group by name.
 */
export function defaultAccessRows(
  tools: readonly DefaultAccessTool[],
  families: readonly string[],
  access: readonly string[],
): DefaultAccessRow[] {
  const rows = tools
    .filter((tool) => tool.family !== "exempt" && tool.name !== CODE_CONTEXT_TOOL)
    .map((tool): DefaultAccessRow => {
      const byFamily = families.includes(tool.family);
      return {
        id: tool.name,
        checked: byFamily || access.includes(tool.name),
        sensitive: isSensitiveFamily(tool.family),
        byFamily,
      };
    });
  const rank = (row: DefaultAccessRow): number => (row.byFamily ? 0 : row.sensitive ? 2 : 1);
  return rows.sort((a, b) => rank(a) - rank(b) || a.id.localeCompare(b.id));
}

/** The named accesses once `id` is checked or unchecked, without duplicate. */
export function toggledAccess(access: readonly string[], id: string, on: boolean): string[] {
  const without = access.filter((name) => name !== id);
  return on ? [...without, id] : without;
}

/** Whether a line can be checked or unchecked here. */
export function rowLocked(row: DefaultAccessRow, source: SettingSource): boolean {
  return row.byFamily || source === "policy";
}

/** One standing consent, as the summary needs it. */
export interface AlwaysConsent {
  agent_id: string;
}

/** One line of the summary: an agent and how many accesses it never asks for. */
export interface AlwaysLine {
  agentId: string;
  name: string;
  count: number;
}

/**
 * The summary of what is always authorized: one line per agent holding at
 * least one standing consent, by name. An agent nobody names shows its
 * identifier.
 */
export function alwaysSummary(
  consents: readonly AlwaysConsent[],
  nameOf: (agentId: string) => string | undefined,
): AlwaysLine[] {
  const counts = new Map<string, number>();
  for (const consent of consents) {
    counts.set(consent.agent_id, (counts.get(consent.agent_id) ?? 0) + 1);
  }
  return [...counts.entries()]
    .map(([agentId, count]) => ({ agentId, name: nameOf(agentId) ?? agentId, count }))
    .sort((a, b) => a.name.localeCompare(b.name));
}

/** The strings of a setting read as a list; anything else reads as empty. */
export function stringsOf(value: unknown): string[] {
  return Array.isArray(value) ? value.filter((v): v is string => typeof v === "string") : [];
}

/** The four defaults of the section, each with the source of its value. */
export interface IntegrationDefaults {
  families: string[];
  access: string[];
  accessSource: SettingSource;
  shareProfile: boolean;
  shareProfileSource: SettingSource;
  allowRequestMcp: boolean;
  allowRequestMcpSource: SettingSource;
}

/** The defaults before the shell answers: the ones of a fresh installation. */
export const INTEGRATION_DEFAULTS: IntegrationDefaults = {
  families: ["read"],
  access: [],
  accessSource: "default",
  shareProfile: true,
  shareProfileSource: "default",
  allowRequestMcp: false,
  allowRequestMcpSource: "default",
};

/** The two shell calls the section makes, replaceable in tests. */
export interface SettingsIpc {
  read: (key: string) => Promise<ResolvedSetting>;
  write: (key: string, value: unknown) => Promise<ResolvedSetting>;
}

export const shellSettingsIpc: SettingsIpc = { read: getResolvedSetting, write: setSetting };

/**
 * Reads the defaults in force. A key the shell cannot answer keeps the value
 * of a fresh installation: the section never shows a default wider than the
 * one it could read.
 */
export async function readIntegrationDefaults(
  ipc: SettingsIpc = shellSettingsIpc,
): Promise<IntegrationDefaults> {
  const read = async (key: string): Promise<ResolvedSetting | null> => {
    try {
      return await ipc.read(key);
    } catch {
      return null;
    }
  };
  const [families, access, share, mcp] = await Promise.all([
    read(NEW_AGENT_FAMILIES_KEY),
    read(NEW_AGENT_ACCESS_KEY),
    read(SHARE_PROFILE_DEFAULT_KEY),
    read(ALLOW_REQUEST_MCP_KEY),
  ]);
  const base = INTEGRATION_DEFAULTS;
  return {
    families: families ? stringsOf(families.value) : base.families,
    access: access ? stringsOf(access.value) : base.access,
    accessSource: access?.source ?? base.accessSource,
    shareProfile: share ? share.value !== false : base.shareProfile,
    shareProfileSource: share?.source ?? base.shareProfileSource,
    allowRequestMcp: mcp ? mcp.value === true : base.allowRequestMcp,
    allowRequestMcpSource: mcp?.source ?? base.allowRequestMcpSource,
  };
}

/**
 * Checks or unchecks one access of a new agent and answers the defaults that
 * result. A refused write (a value the policy imposes, an unreadable file)
 * throws and `defaults` stays as it was.
 */
export async function writeDefaultAccess(
  defaults: IntegrationDefaults,
  id: string,
  on: boolean,
  ipc: SettingsIpc = shellSettingsIpc,
): Promise<IntegrationDefaults> {
  const written = await ipc.write(NEW_AGENT_ACCESS_KEY, toggledAccess(defaults.access, id, on));
  return { ...defaults, access: stringsOf(written.value), accessSource: written.source };
}

/** Writes the profile switch a new conversation is born with. */
export async function writeShareProfile(
  defaults: IntegrationDefaults,
  on: boolean,
  ipc: SettingsIpc = shellSettingsIpc,
): Promise<IntegrationDefaults> {
  const written = await ipc.write(SHARE_PROFILE_DEFAULT_KEY, on);
  return {
    ...defaults,
    shareProfile: written.value !== false,
    shareProfileSource: written.source,
  };
}

/** Writes whether a request may bring an MCP server of its own. */
export async function writeAllowRequestMcp(
  defaults: IntegrationDefaults,
  on: boolean,
  ipc: SettingsIpc = shellSettingsIpc,
): Promise<IntegrationDefaults> {
  const written = await ipc.write(ALLOW_REQUEST_MCP_KEY, on);
  return {
    ...defaults,
    allowRequestMcp: written.value === true,
    allowRequestMcpSource: written.source,
  };
}

/** Opens the Authorizations screen, where a standing consent is withdrawn. */
export function openAuthorizations(): void {
  navigateTo("authorizations");
}

/** Opens the Connections screen, where accounts and services are plugged in. */
export function openConnections(): void {
  navigateTo("integrations");
}
