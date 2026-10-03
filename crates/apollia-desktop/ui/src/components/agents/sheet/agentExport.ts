/**
 * The logic of the export dialog of an agent, kept out of the component so it
 * can be tested without rendering it.
 */
import type { ResolvedAgent } from "$lib/ipc/agentSheet";

/** Setting that holds the publisher name of the exports of this machine. */
export const AUTHOR_NAME_KEY = "author.name";

/** Version the runtime writes when the agent's own is not semver. */
const FALLBACK_VERSION = "1.0.0";

const SEMVER = /^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/;

/** Whether `agent` can be exported: only an agent created in the interface. */
export function canExport(agent: ResolvedAgent | null): boolean {
  return agent !== null && agent.origin === "interface";
}

/** The file name the runtime offers for `agent`: `<id>-<version>.apo`. */
export function exportFileName(agent: Pick<ResolvedAgent, "id" | "version">): string {
  const version = SEMVER.test(agent.version) ? agent.version : FALLBACK_VERSION;
  return `${agent.id}-${version}.apo`;
}

/** The publisher name as typed, trimmed; `null` when blank. */
export function publisherOf(typed: string): string | null {
  const name = typed.trim();
  return name === "" ? null : name;
}

/** A fingerprint shown in groups of four, the first sixteen characters. */
export function shortFingerprint(fingerprint: string | null): string {
  if (!fingerprint) return "";
  return (
    fingerprint
      .slice(0, 16)
      .toUpperCase()
      .match(/.{1,4}/g)
      ?.join(" ") ?? ""
  );
}
