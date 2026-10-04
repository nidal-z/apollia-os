/**
 * The profile the onboarding agent proposes, between the conversation and the
 * form the person reads it over in.
 *
 * The agent writes nothing: its tool `propose_profile` answers with a
 * proposal, which stays in the conversation as the output of that call. These
 * helpers read it there, lay it over the fields of a profile, and write the
 * profile only when the person saves. Kept out of the Svelte components so
 * the decisions run under Vitest (node environment).
 */
import type { ProfileFieldView, ProfileUpdate } from "$lib/ipc/profile";
import type { ChatMessageView } from "$lib/types";

/** The tool of the onboarding agent, as its package names it. */
export const PROPOSAL_TOOL = "propose_profile";

/** A proposed profile: field key to value, only the fields the person spoke about. */
export type ProfileProposal = Record<string, string>;

/** The profile a conversation is asked to complete, as Settings > Profile hands it. */
export interface EnrichmentRequest {
  profileId: string;
  fields: ProfileFieldView[];
}

/** A proposal on its way back to Settings > Profile, for the profile it was asked for. */
export interface PendingProposal {
  profileId: string;
  proposal: ProfileProposal;
}

function isToolCall(name: string): boolean {
  return name === PROPOSAL_TOOL || name.endsWith(`/${PROPOSAL_TOOL}`);
}

function proposalOf(output: string | null): ProfileProposal | null {
  if (!output) return null;
  let parsed: unknown;
  try {
    parsed = JSON.parse(output);
  } catch {
    return null;
  }
  if (typeof parsed !== "object" || parsed === null) return null;
  // The answer of the tool, or the same answer under the `result` key some
  // executors wrap it in.
  const answer = parsed as { proposal?: unknown; result?: { proposal?: unknown } };
  const raw = answer.proposal ?? answer.result?.proposal;
  if (typeof raw !== "object" || raw === null || Array.isArray(raw)) return null;
  const out: ProfileProposal = {};
  for (const [key, value] of Object.entries(raw)) {
    if (typeof value === "string" && value.trim()) out[key] = value.trim();
  }
  return out;
}

/**
 * The latest proposal of a conversation, or `null` when the agent has not
 * proposed yet. Only a call that ran counts: a refused or failed call
 * proposes nothing.
 */
export function extractProposal(messages: ChatMessageView[]): ProfileProposal | null {
  for (let m = messages.length - 1; m >= 0; m -= 1) {
    const calls = messages[m].tool_calls ?? [];
    for (let c = calls.length - 1; c >= 0; c -= 1) {
      const call = calls[c];
      if (!isToolCall(call.tool_name) || call.status !== "executed") continue;
      const proposal = proposalOf(call.output);
      if (proposal) return proposal;
    }
  }
  return null;
}

/**
 * The values the review form opens with: what the profile holds, with the
 * proposed value laid over each field the agent proposed. A proposed key the
 * profile does not declare is dropped.
 */
export function reviewValues(
  fields: ProfileFieldView[],
  proposal: ProfileProposal | null,
): Record<string, string> {
  return Object.fromEntries(
    fields.map((f) => [f.key, proposal?.[f.key]?.trim() || f.value]),
  );
}

/** Whether a proposal would change anything the profile holds. */
export function proposalChanges(
  fields: ProfileFieldView[],
  proposal: ProfileProposal | null,
): boolean {
  const values = reviewValues(fields, proposal);
  return fields.some((f) => values[f.key].trim() !== f.value.trim());
}

/**
 * Lay a proposal handed back to Settings > Profile over the form of the
 * displayed profile: the proposed fields replace what the form shows, the
 * others keep the edits in progress. Returns the new form values, or `null`
 * when there is nothing to review: the proposal was asked for another
 * profile, or it changes nothing. The profile itself is not written.
 */
export function layProposal(
  displayedId: string,
  fields: ProfileFieldView[],
  values: Record<string, string>,
  pending: PendingProposal,
): Record<string, string> | null {
  if (pending.profileId !== displayedId) return null;
  if (!proposalChanges(fields, pending.proposal)) return null;
  const proposed = reviewValues(fields, pending.proposal);
  const next = { ...values };
  for (const field of fields) {
    if (pending.proposal[field.key]?.trim()) next[field.key] = proposed[field.key];
  }
  return next;
}

/**
 * What the person already filled in, as one line the agent reads in the first
 * message so it asks only about the rest; empty when the profile is empty.
 */
export function knownFieldsLine(
  fields: ProfileFieldView[],
  label: (field: ProfileFieldView) => string,
): string {
  return fields
    .filter((f) => f.value.trim())
    .map((f) => `${label(f)}: ${f.value.trim().replace(/\s+/g, " ")}`)
    .join("; ");
}

/** What a review reaches. */
export interface ReviewEffects {
  update: (id: string, update: ProfileUpdate) => Promise<unknown>;
}

/** How a review ends: the person saves what they read, or leaves without saving. */
export type ReviewChoice = "save" | "skip";

/**
 * End a review. "skip" reaches no effect at all: nothing is written. "save"
 * writes the reviewed values to the profile, only the fields that differ from
 * what it holds; a review that changes nothing writes nothing either.
 * Returns whether a write happened.
 */
export async function applyReview(
  choice: ReviewChoice,
  profileId: string,
  fields: ProfileFieldView[],
  values: Record<string, string>,
  effects: ReviewEffects,
): Promise<boolean> {
  if (choice === "skip") return false;
  const changed: Record<string, string> = {};
  for (const field of fields) {
    const next = (values[field.key] ?? "").trim();
    if (next !== field.value.trim()) changed[field.key] = next;
  }
  if (Object.keys(changed).length === 0) return false;
  await effects.update(profileId, { fields: changed });
  return true;
}
