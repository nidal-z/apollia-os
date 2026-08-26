/**
 * The delegation a free chat hands to a worker agent, while it runs.
 *
 * Two runtime-event categories feed it: `a2a` opens and closes the delegation,
 * `task-changed` reports the sub-agent's steps. Both are subscribed together
 * because a step that arrives with no delegation open belongs to somebody else's
 * task and must be dropped, and only this pair knows that.
 */
import { get } from "svelte/store";
import { t } from "svelte-i18n";

/** One step the delegated agent reported. */
export interface A2AStepView {
  step_id: string;
  step_num: number;
  total: number;
  desc: string;
  status: "running" | "done" | "failed";
  durationMs?: number;
}

/** The shape the two `runtime-event` listeners receive. */
export interface RuntimeEventPayload {
  category: string;
  event_type: string;
  payload: Record<string, unknown>;
}

export interface A2ADelegation {
  /** Non-null while a delegation is in progress. */
  readonly active: { target: string; skill_id: string } | null;
  readonly steps: A2AStepView[];
  /** Guard trigger message from A2A guardrails, or a failure sentence. */
  readonly guardMessage: string | null;
  /** Elapsed seconds of the current delegation, refreshed every second. */
  readonly elapsed: number;
  /** True while the duration timer should run. */
  readonly running: boolean;
  /** Refresh `elapsed` from the recorded start. */
  tick(): void;
  /** Handle one `a2a` runtime event. */
  onLifecycleEvent(event: RuntimeEventPayload, onChange: () => void): void;
  /** Handle one `task-changed` runtime event. */
  onStepEvent(event: RuntimeEventPayload, onChange: () => void): void;
  /** Drop everything, e.g. when the conversation is switched. */
  reset(): void;
}

export function createA2ADelegation(): A2ADelegation {
  let active = $state<{ target: string; skill_id: string } | null>(null);
  let steps = $state<A2AStepView[]>([]);
  let guardMessage = $state<string | null>(null);
  let startTime = $state<number | null>(null);
  let elapsed = $state(0);

  function reset(): void {
    active = null;
    steps = [];
    guardMessage = null;
    startTime = null;
    elapsed = 0;
  }

  return {
    get active() {
      return active;
    },
    get steps() {
      return steps;
    },
    get guardMessage() {
      return guardMessage;
    },
    get elapsed() {
      return elapsed;
    },
    get running() {
      return startTime !== null;
    },
    tick(): void {
      if (startTime) elapsed = Math.round((Date.now() - startTime) / 1000);
    },
    reset,

    onLifecycleEvent(event: RuntimeEventPayload, onChange: () => void): void {
      if (event.category !== "a2a") return;
      if (event.event_type === "A2AInvocationStarted") {
        const p = event.payload as { caller?: string; target?: string; skill_id?: string };
        if (p.caller === "chat-libre") {
          active = { target: p.target ?? "", skill_id: p.skill_id ?? "" };
          steps = [];
          guardMessage = null;
          startTime = Date.now();
          elapsed = 0;
          onChange();
        }
      } else if (event.event_type === "A2AInvocationCompleted") {
        const p = event.payload as { status?: string; duration_ms?: number };
        // Brief delay to show final status before clearing
        const finalStatus = p.status ?? "completed";
        const finalDuration = p.duration_ms;
        if (finalStatus === "failed" && active) {
          guardMessage = get(t)("chat.a2a.delegation_failed", {
            values: {
              duration: finalDuration
                ? `${finalDuration}ms`
                : get(t)("chat.a2a.unknown_duration"),
            },
          });
        }
        setTimeout(reset, finalStatus === "failed" ? 2000 : 300);
      } else if (event.event_type === "A2AGuardTriggered") {
        const p = event.payload as { detail?: string; guard_type?: string };
        guardMessage =
          p.detail ??
          get(t)("chat.a2a.guard_triggered", { values: { type: p.guard_type ?? "" } });
        onChange();
      }
    },

    onStepEvent(event: RuntimeEventPayload, onChange: () => void): void {
      if (event.category !== "task-changed") return;
      if (!active) return;

      if (event.event_type === "StepStarted") {
        const p = event.payload as { step_id?: string; step_num?: number; total?: number; desc?: string };
        steps = [
          ...steps,
          {
            step_id: p.step_id ?? `s${steps.length}`,
            step_num: p.step_num ?? steps.length + 1,
            total: p.total ?? 0,
            desc: p.desc ?? "",
            status: "running",
          },
        ];
        onChange();
      } else if (event.event_type === "StepCompleted") {
        const p = event.payload as { step_id?: string; duration_ms?: number };
        steps = steps.map((s) =>
          s.step_id === p.step_id ? { ...s, status: "done" as const, durationMs: p.duration_ms } : s,
        );
      } else if (event.event_type === "StepFailed") {
        const p = event.payload as { step_id?: string; error?: string };
        steps = steps.map((s) =>
          s.step_id === p.step_id ? { ...s, status: "failed" as const } : s,
        );
      }
    },
  };
}
