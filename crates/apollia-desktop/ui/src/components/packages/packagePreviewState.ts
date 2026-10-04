/**
 * What the preview of a package shows, decided from the runtime's answer.
 *
 * The dialog renders this and nothing else: which variant it is (install,
 * unsigned, refused for an invalid signature, refused by the organisation
 * policy, refused for another reason, update), the content grouped by kind,
 * the accesses grouped by action family, the publisher, the connections, what
 * an update adds and must have accepted, and whether the main action is
 * enabled. Kept free of Svelte so the decisions are tested on their own.
 */
import type {
  AccessPreview,
  InstallRefusal,
  PackagePreview,
} from "$lib/ipc/packageFiles";

/** The shape of the dialog. */
export type PreviewVariant =
  | "install"
  | "unsigned"
  | "invalid"
  | "policy_refused"
  | "refused"
  | "update";

/** How the publisher reads. */
export type PublisherState = "trusted" | "signed_untrusted" | "unsigned" | "invalid";

/** The action families, in the order the dialog lists them. */
export const FAMILY_ORDER = [
  "read",
  "read_personal",
  "write_or_delete",
  "send_external",
  "execute_code",
  "unclassified",
] as const;

export type Family = (typeof FAMILY_ORDER)[number];

/** One line of the content of the package. */
export interface ContentLine {
  kind: "agent" | "workflow" | "tool";
  name: string;
  summary: string;
}

/** The accesses of one family, each asked once whichever agents ask it. */
export interface FamilyGroup {
  family: Family;
  accesses: AccessPreview[];
}

/** One connection the package requires or brings. */
export interface ConnectionLine {
  name: string;
  reason: string;
  state: "missing" | "present" | "brought";
}

/** Something an update adds, which the user accepts before updating. */
export interface ChangeToAccept {
  /** Stable key of the acceptance. */
  key: string;
  kind: "access" | "connection";
  /** What it is: the access permits, or the connection name. */
  label: string;
  /** Family of an access, `null` for a connection. */
  family: Family | null;
  /** A new connection that this machine does not have yet. */
  missing: boolean;
}

/** Which variant the preview of `preview` is. */
export function previewVariant(preview: PackagePreview): PreviewVariant {
  const refusal = preview.refusal ?? null;
  if (refusal !== null) {
    if (refusal.code === "invalid_signature") return "invalid";
    if (refusal.code === "signature_required" || refusal.code === "publisher_not_trusted") {
      return "policy_refused";
    }
    return "refused";
  }
  if (preview.signature.status === "invalid") return "invalid";
  if (preview.update) return "update";
  if (preview.signature.status === "unsigned") return "unsigned";
  return "install";
}

/** How the publisher of `preview` reads. */
export function publisherState(preview: PackagePreview): PublisherState {
  const { status, trusted } = preview.signature;
  if (status === "invalid") return "invalid";
  if (status === "unsigned") return "unsigned";
  return trusted ? "trusted" : "signed_untrusted";
}

/** The publisher can be trusted from the preview: signed, not yet trusted, key known. */
export function canTrustPublisher(preview: PackagePreview): boolean {
  return publisherState(preview) === "signed_untrusted" && !!preview.signature.publicKey;
}

/** The agents, workflows and tools of the package, in that order. */
export function contentLines(preview: PackagePreview): ContentLine[] {
  return [
    ...preview.agents.map((a) => ({ kind: "agent" as const, name: a.name || a.id, summary: a.mission })),
    ...preview.workflows.map((w) => ({ kind: "workflow" as const, name: w.name, summary: w.description })),
    ...preview.tools.map((t) => ({ kind: "tool" as const, name: t.name, summary: t.description })),
  ];
}

/** The count of each kind of content. */
export function contentCounts(preview: PackagePreview): Record<ContentLine["kind"], number> {
  return {
    agent: preview.agents.length,
    workflow: preview.workflows.length,
    tool: preview.tools.length,
  };
}

/** `family` as one of the listed families; anything unknown is unclassified. */
export function asFamily(family: string): Family {
  return (FAMILY_ORDER as readonly string[]).includes(family) ? (family as Family) : "unclassified";
}

/** The accesses the agents ask for, each once, grouped by family in order. */
export function accessGroups(preview: PackagePreview): FamilyGroup[] {
  const seen = new Map<string, AccessPreview>();
  for (const agent of preview.agents) {
    for (const access of agent.access) {
      if (!seen.has(access.accessId)) seen.set(access.accessId, access);
    }
  }
  const groups: FamilyGroup[] = [];
  for (const family of FAMILY_ORDER) {
    const accesses = [...seen.values()].filter((a) => asFamily(a.family) === family);
    if (accesses.length > 0) groups.push({ family, accesses });
  }
  return groups;
}

/**
 * The connections: each required one, present or missing, then each one the
 * package brings. A required connection the package brings itself is listed
 * once, as brought.
 */
export function connectionLines(preview: PackagePreview): ConnectionLine[] {
  const lines: ConnectionLine[] = preview.requires
    .filter((r) => !r.broughtByPackage)
    .map((r) => ({ name: r.type, reason: r.reason, state: r.present ? "present" : "missing" }));
  for (const server of preview.mcpServers) {
    const required = preview.requires.find(
      (r) => r.broughtByPackage && r.type.toLowerCase() === server.name.toLowerCase(),
    );
    lines.push({ name: server.name, reason: required?.reason ?? "", state: "brought" });
  }
  return lines;
}

/** What an update adds and must have accepted; empty outside an update. */
export function changesToAccept(preview: PackagePreview): ChangeToAccept[] {
  const update = preview.update;
  if (!update) return [];
  const changes: ChangeToAccept[] = [];
  const seen = new Set<string>();
  for (const access of update.addedAccess) {
    const key = `access:${access.accessId}`;
    if (seen.has(key)) continue;
    seen.add(key);
    changes.push({
      key,
      kind: "access",
      label: access.permits,
      family: asFamily(access.family),
      missing: false,
    });
  }
  for (const name of [...update.addedRequires, ...update.addedMcpServers]) {
    const key = `connection:${name.toLowerCase()}`;
    if (seen.has(key)) continue;
    seen.add(key);
    const required = preview.requires.find((r) => r.type.toLowerCase() === name.toLowerCase());
    const missing = required !== undefined && !required.present && !required.broughtByPackage;
    changes.push({ key, kind: "connection", label: name, family: null, missing });
  }
  return changes;
}

/** Every change of `changes` is in `accepted`. */
export function allAccepted(changes: ChangeToAccept[], accepted: ReadonlySet<string>): boolean {
  return changes.every((c) => accepted.has(c.key));
}

/** The main action is enabled: an install or update not refused, every change accepted. */
export function canProceed(preview: PackagePreview, accepted: ReadonlySet<string>): boolean {
  const variant = previewVariant(preview);
  if (variant === "invalid" || variant === "policy_refused" || variant === "refused") {
    return false;
  }
  return allAccepted(changesToAccept(preview), accepted);
}

/** What the installation confirms to the runtime. */
export function installConfirmations(
  preview: PackagePreview,
  accepted: ReadonlySet<string>,
): { depsConfirmed: boolean; changesConfirmed: boolean } {
  return {
    depsConfirmed: preview.pip.length > 0,
    changesConfirmed: preview.update ? allAccepted(changesToAccept(preview), accepted) : false,
  };
}

/** The refusal of `preview`, `null` when it installs. */
export function refusalOf(preview: PackagePreview): InstallRefusal | null {
  return preview.refusal ?? null;
}

/** The file name at the end of `path`, on either separator. */
export function fileName(path: string): string {
  const parts = path.split(/[\\/]/);
  return parts[parts.length - 1] || path;
}

/** The detail a user copies to report a refusal: package, file and reason. */
export function refusalDetail(preview: PackagePreview, path: string, reason: string): string {
  const lines = [`${preview.name} ${preview.version}`, path, reason];
  if (preview.signature.keyFingerprint) lines.push(preview.signature.keyFingerprint);
  return lines.join("\n");
}
