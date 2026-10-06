/**
 * What the Library screen decides, apart from how it renders: the entries of
 * its list, their filter and tally, the content of an open package, and what
 * an uninstallation removes, keeps and asks.
 *
 * The packages Apollia ships inside the application are shown as one entry,
 * "provided with Apollia": they have no publisher to name, they update with
 * the application, and they are never uninstalled.
 */
import type {
  AgentPackageDetailView,
  AgentPackageListItem,
  PackageAutomation,
  PackageContentView,
  PackageCounts,
} from "$lib/types";

/** Key of the entry that stands for the packages provided with Apollia. */
export const PROVIDED_ENTRY = "@provided";

/** One line of the Library list. */
export interface LibraryEntry {
  /** The package name, or `PROVIDED_ENTRY`. */
  key: string;
  /** The entry stands for the packages provided with Apollia. */
  provided: boolean;
  /** Publisher shown; empty for the provided entry, which the screen names. */
  publisher: string;
  version: string;
  /**
   * The package carried a signature that held. The provided entry counts as
   * signed: the application that carries it is what the distribution signs.
   */
  signed: boolean;
  counts: PackageCounts;
  /** Names of the installed packages behind the entry. */
  packages: string[];
}

function addCounts(a: PackageCounts, b: PackageCounts): PackageCounts {
  return {
    agents: a.agents + b.agents,
    workflows: a.workflows + b.workflows,
    tools: a.tools + b.tools,
    connections: a.connections + b.connections,
  };
}

/** The versions of `packages`, each once, in the order met. */
function versionsOf(packages: readonly { version: string }[]): string {
  return [...new Set(packages.map((p) => p.version))].join(", ");
}

/**
 * The entries of the list: the provided entry first, when Apollia ships at
 * least one package, then the installed packages by name.
 */
export function libraryEntries(packages: readonly AgentPackageListItem[]): LibraryEntry[] {
  const provided = packages.filter((p) => p.embedded);
  const installed = packages
    .filter((p) => !p.embedded)
    .sort((a, b) => a.name.localeCompare(b.name));
  const entries: LibraryEntry[] = [];
  if (provided.length > 0) {
    entries.push({
      key: PROVIDED_ENTRY,
      provided: true,
      publisher: "",
      version: versionsOf(provided),
      signed: true,
      counts: provided
        .map((p) => p.counts)
        .reduce(addCounts, { agents: 0, workflows: 0, tools: 0, connections: 0 }),
      packages: provided.map((p) => p.name),
    });
  }
  for (const pkg of installed) {
    entries.push({
      key: pkg.name,
      provided: false,
      publisher: pkg.signature.publisher,
      version: pkg.version,
      signed: pkg.signature.signed,
      counts: pkg.counts,
      packages: [pkg.name],
    });
  }
  return entries;
}

/**
 * The entries whose name or publisher contains `query`, case ignored. The
 * provided entry is matched on `providedLabel`, the name the screen gives it.
 */
export function filterEntries(
  entries: readonly LibraryEntry[],
  query: string,
  providedLabel: string,
): LibraryEntry[] {
  const q = query.trim().toLowerCase();
  if (q.length === 0) return [...entries];
  return entries.filter((entry) => {
    const name = entry.provided ? providedLabel : entry.key;
    return name.toLowerCase().includes(q) || entry.publisher.toLowerCase().includes(q);
  });
}

/** How many entries there are, and how many are signed or not. */
export function signatureTally(entries: readonly LibraryEntry[]): {
  total: number;
  signed: number;
  unsigned: number;
} {
  const signed = entries.filter((e) => e.signed).length;
  return { total: entries.length, signed, unsigned: entries.length - signed };
}

/** An open package, or the packages provided with Apollia as one. */
export interface LibraryDetail {
  key: string;
  provided: boolean;
  description: string;
  publisher: string;
  version: string;
  signed: boolean;
  trusted: boolean;
  keyFingerprint: string | null;
  /** Date of installation, as recorded; the earliest for the provided entry. */
  installedAt: string;
  /** Each agent and workflow with the package it belongs to. */
  content: PackageContentView;
  /** The package each workflow belongs to, by workflow id. */
  workflowPackage: Record<string, string>;
  automations: PackageAutomation[];
}

/** The open package `detail`, as the screen shows it. */
export function packageDetail(detail: AgentPackageDetailView): LibraryDetail {
  return {
    key: detail.name,
    provided: false,
    description: detail.description,
    publisher: detail.signature.publisher,
    version: detail.version,
    signed: detail.signature.signed,
    trusted: detail.signature.trusted,
    keyFingerprint: detail.signature.key_fingerprint,
    installedAt: detail.installed_at,
    content: detail.content,
    workflowPackage: Object.fromEntries(detail.content.workflows.map((w) => [w.id, detail.name])),
    automations: detail.automations ?? [],
  };
}

/** The packages provided with Apollia, shown as one open package. */
export function providedDetail(details: readonly AgentPackageDetailView[]): LibraryDetail {
  const content: PackageContentView = {
    agents: details.flatMap((d) => d.content.agents),
    workflows: details.flatMap((d) => d.content.workflows),
    tools: details.flatMap((d) => d.content.tools),
    connections_brought: details.flatMap((d) => d.content.connections_brought),
    connections_required: details.flatMap((d) => d.content.connections_required),
  };
  const dates = details.map((d) => d.installed_at).filter((d) => d.length > 0).sort();
  return {
    key: PROVIDED_ENTRY,
    provided: true,
    description: "",
    publisher: "",
    version: versionsOf(details),
    signed: true,
    trusted: true,
    keyFingerprint: null,
    installedAt: dates[0] ?? "",
    content,
    workflowPackage: Object.fromEntries(
      details.flatMap((d) => d.content.workflows.map((w) => [w.id, d.name])),
    ),
    automations: details.flatMap((d) => d.automations ?? []),
  };
}

/** The kinds a package can hold, in the order the screen lists them. */
export type ContentKind = "agent" | "workflow" | "tool" | "connection";

/** The kinds `content` holds nothing of. */
export function emptyKinds(content: PackageContentView): ContentKind[] {
  const kinds: ContentKind[] = [];
  if (content.agents.length === 0) kinds.push("agent");
  if (content.workflows.length === 0) kinds.push("workflow");
  if (content.tools.length === 0) kinds.push("tool");
  if (content.connections_brought.length + content.connections_required.length === 0) {
    kinds.push("connection");
  }
  return kinds;
}

/**
 * `labels` as one enumeration: commas, then `last` before the final one
 * ("workflow, outil ni connexion").
 */
export function enumerate(labels: readonly string[], last: string): string {
  if (labels.length <= 1) return labels.join("");
  return `${labels.slice(0, -1).join(", ")} ${last} ${labels.at(-1)}`;
}

/** What goes with a package, by kind. */
export type LeavingKind = "agents" | "workflows" | "tools" | "automations" | "connections";

/** One line of "what goes": a kind and the names it covers. */
export interface LeavingLine {
  kind: LeavingKind;
  names: string[];
}

/**
 * What the uninstallation of `detail` removes, one line per kind that holds
 * something: its agents, workflows and tools, the automations that start its
 * agents or workflows, and the connections it brought.
 */
export function whatGoes(detail: LibraryDetail): LeavingLine[] {
  const lines: LeavingLine[] = [
    { kind: "agents", names: detail.content.agents.map((a) => a.name) },
    { kind: "workflows", names: detail.content.workflows.map((w) => w.name) },
    { kind: "tools", names: detail.content.tools.map((t) => t.name) },
    { kind: "automations", names: detail.automations.map((a) => a.id) },
    { kind: "connections", names: [...detail.content.connections_brought] },
  ];
  return lines.filter((line) => line.names.length > 0);
}

/** The connections the package requires: they belong to the user and stay. */
export function whatStays(detail: LibraryDetail): string[] {
  return detail.content.connections_required.map((c) => c.type);
}

/** What the user chose for the memory and the history of the agents. */
export type DataChoice = "keep" | "erase";

/** The identifier a workflow of `detail` is known by outside its package. */
export function workflowTarget(detail: LibraryDetail, workflowId: string): string {
  const pkg = detail.workflowPackage[workflowId] ?? detail.key;
  return `${pkg}/${workflowId}`;
}

/** `iso` as a date of `locale`, or empty when it is not a date. */
export function installedOn(iso: string, locale: string): string {
  const date = new Date(iso);
  if (iso.length === 0 || Number.isNaN(date.getTime())) return "";
  return date.toLocaleDateString(locale, { year: "numeric", month: "long", day: "numeric" });
}
