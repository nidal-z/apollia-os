/**
 * Whether a model can answer the guide.
 *
 * The guide's session runs on the models the chat can talk to: the language
 * models installed on this machine, which load at the first question, and the
 * models a connected server offers. With none of either, no question can be
 * answered, and the Guide tab shows the documentation of the screen instead.
 */
import type { ChatTargetsState } from "$lib/stores/chatTargets";

/** `unknown` until the models were read once. */
export type GuideModelState = "unknown" | "available" | "none";

/** The state the list of the chat's models puts the guide in. */
export function guideModelState(targets: ChatTargetsState): GuideModelState {
  if (targets.groups.models.length > 0) return "available";
  return targets.loaded ? "none" : "unknown";
}
