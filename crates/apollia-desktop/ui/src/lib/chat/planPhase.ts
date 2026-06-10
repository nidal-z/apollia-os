// Maps a conversational plan-mode phase to its i18n label key.
//
// The phase is the stable lowercase string carried by the runtime
// (`ChatPlanPhaseChanged` / `ChatSessionDetail.plan_phase`). Keeping the
// mapping in one place lets the review components stay free of phase logic.

import type { PlanPhase } from "$lib/types";

const PHASE_LABEL_KEYS: Record<PlanPhase, string> = {
  discovery: "chat.planMode.phase.discovery",
  drafting: "chat.planMode.phase.drafting",
  awaiting_approval: "chat.planMode.phase.awaitingApproval",
  executing: "chat.planMode.phase.executing",
  done: "chat.planMode.phase.done",
};

/** Returns the i18n key for a plan-mode phase label. */
export function phaseLabelKey(phase: PlanPhase): string {
  return PHASE_LABEL_KEYS[phase] ?? PHASE_LABEL_KEYS.done;
}
