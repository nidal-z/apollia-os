/**
 * The organisation policy in plain words: which sections it touches and, per
 * section, one line per rule it imposes. The runtime serves the policy as
 * structure; the sentences are written here, through i18n keys, so the screen
 * speaks the language of the person reading it.
 */
import type { OrgPolicyBody, PolicyInvalidReason } from "$lib/ipc/policy";

/** One rule: what is imposed (a label) and its value. */
export interface PolicyRuleLine {
  /** Stable id, for the `data-testid` of the row. */
  id: string;
  /** i18n key of what is imposed. */
  labelKey?: string;
  /** Literal label, for a setting key that has no sentence of its own. */
  label?: string;
  /** i18n key of the value. */
  valueKey?: string;
  /** Values interpolated in `valueKey`. */
  valueValues?: Record<string, string | number>;
  /** Literal value (identifiers, addresses, folders). */
  value?: string;
  /** Rendered in a fixed-width font: an identifier, an address, a path. */
  code?: boolean;
}

/** The sections of a policy, in the order the screen shows them. */
export type PolicySectionId =
  | "connections"
  | "families"
  | "inference"
  | "settings"
  | "packages"
  | "journal";

export interface PolicySectionModel {
  id: PolicySectionId;
  rules: PolicyRuleLine[];
}

function list(items: readonly string[]): string {
  return items.join(", ");
}

function settingValue(value: unknown): string {
  return typeof value === "string" ? value : JSON.stringify(value);
}

/** The rules of `policy`, section by section; a section that imposes nothing is left out. */
export function policySections(policy: OrgPolicyBody): PolicySectionModel[] {
  const connections: PolicyRuleLine[] = [];
  const c = policy.connections;
  if (c.allow.length > 0) {
    connections.push({ id: "connections-allow", labelKey: "policy.rule.connections_allow", value: list(c.allow), code: true });
  }
  if (c.deny.length > 0) {
    connections.push({ id: "connections-deny", labelKey: "policy.rule.connections_deny", value: list(c.deny), code: true });
  }
  if (c.remote === "deny") {
    connections.push({ id: "connections-remote", labelKey: "policy.rule.connections_remote", valueKey: "policy.value.refused" });
  }

  const families: PolicyRuleLine[] = policy.families.deny.map((family) => ({
    id: `family-${family}`,
    labelKey: `agent_sheet.family_${family}`,
    valueKey: "policy.value.forbidden",
  }));

  const inference: PolicyRuleLine[] = [];
  const i = policy.inference;
  if (i.allowed_servers.length > 0) {
    inference.push({ id: "inference-servers", labelKey: "policy.rule.inference_servers", value: list(i.allowed_servers), code: true });
  }
  if (i.remote_endpoints === "deny") {
    inference.push({ id: "inference-remote", labelKey: "policy.rule.inference_remote", valueKey: "policy.value.refused_feminine" });
  }
  if (i.require_model_hash) {
    inference.push({ id: "inference-hash", labelKey: "policy.rule.inference_hash", valueKey: "policy.value.refused" });
  }

  const settings: PolicyRuleLine[] = Object.entries(policy.settings).map(([key, value]) => ({
    id: `setting-${key}`,
    label: key,
    value: settingValue(value),
    code: true,
  }));

  const packages: PolicyRuleLine[] = [];
  const p = policy.packages;
  if (p.require_signature) {
    packages.push({ id: "packages-signature", labelKey: "policy.rule.packages_signature", valueKey: "policy.value.required" });
  }
  if (p.trusted_publishers.length > 0) {
    packages.push({ id: "packages-publishers", labelKey: "policy.rule.packages_publishers", value: list(p.trusted_publishers), code: true });
  }

  const journal: PolicyRuleLine[] = [];
  const j = policy.journal;
  if (j.required) {
    journal.push({ id: "journal-required", labelKey: "policy.rule.journal_required", valueKey: "policy.value.mandatory" });
  }
  if (j.retention_days !== null) {
    journal.push({
      id: "journal-retention",
      labelKey: "policy.rule.journal_retention",
      valueKey: "policy.value.days",
      valueValues: { days: j.retention_days },
    });
  }
  if (j.export.detail !== null) {
    journal.push({
      id: "journal-detail",
      labelKey: "policy.rule.journal_detail",
      valueKey: j.export.detail === "forbid" ? "policy.value.never" : "policy.value.always",
    });
  }
  if (j.export.daily_summary_dir !== null) {
    journal.push({ id: "journal-daily", labelKey: "policy.rule.journal_daily", value: j.export.daily_summary_dir, code: true });
  }

  const sections: PolicySectionModel[] = [
    { id: "connections", rules: connections },
    { id: "families", rules: families },
    { id: "inference", rules: inference },
    { id: "settings", rules: settings },
    { id: "packages", rules: packages },
    { id: "journal", rules: journal },
  ];
  return sections.filter((section) => section.rules.length > 0);
}

/** The i18n key of the sentence that says why a policy is refused. */
export function invalidReasonKey(reason: PolicyInvalidReason): string {
  return `policy.reason.${reason}`;
}

/**
 * The signing date of `signedAt` (an RFC 3339 date and time) as a long date in
 * `locale`. The day is the one written in the file, in the offset the
 * organisation signed in; the text as written when it carries no date.
 */
export function formatSignedAt(signedAt: string, locale: string): string {
  const day = /^(\d{4})-(\d{2})-(\d{2})/.exec(signedAt);
  if (!day) return signedAt;
  const date = new Date(Date.UTC(Number(day[1]), Number(day[2]) - 1, Number(day[3])));
  if (Number.isNaN(date.getTime())) return signedAt;
  return new Intl.DateTimeFormat(locale, { dateStyle: "long", timeZone: "UTC" }).format(date);
}
