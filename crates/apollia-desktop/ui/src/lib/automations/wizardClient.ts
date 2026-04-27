/**
 * Wizard client — thin wrapper around the `meta_parse_automation` Tauri IPC
 *
 *
 * Keeps the backend contract (`ParsedAutomation`) localized and exposes a
 * `parseAutomationDescription()` function consumed by `AutomationWizard.svelte`.
 * Falls back to a well-formed `Low`-confidence result on IPC failure so the
 * wizard can still display an actionable message instead of a crash.
 */
import { invoke } from "@tauri-apps/api/core";

export type AutomationConfidence = "high" | "medium" | "low";

export type ParsedSchedule =
  | {
      type: "cron";
      expr: string;
      human_label_fr: string;
      human_label_en: string;
      next_run_at: string;
    }
  | {
      type: "interval";
      every: string;
      every_seconds: number;
      human_label_fr: string;
      human_label_en: string;
      next_run_at: string;
    }
  | {
      type: "one_shot";
      fire_at: string;
      human_label_fr: string;
      human_label_en: string;
      next_run_at: string;
    };

export interface ParsedAgentMatch {
  agent_id: string;
  reason: string;
}

export interface ParsedAutomation {
  schedule: ParsedSchedule | null;
  agent: ParsedAgentMatch | null;
  payload: string | null;
  confidence: AutomationConfidence;
  ambiguities: string[];
}

/**
 * Invoke the backend parser. On error we build a `Low` result so the UI can
 * surface the template-gallery fallback CTA without propagating the IPC noise.
 */
export async function parseAutomationDescription(
  input: string,
  knownAgents: string[],
): Promise<{ ok: true; result: ParsedAutomation } | { ok: false; error: string }> {
  try {
    const result = await invoke<ParsedAutomation>("meta_parse_automation", {
      request: { input, known_agents: knownAgents },
    });
    return { ok: true, result };
  } catch (err) {
    return { ok: false, error: err instanceof Error ? err.message : String(err) };
  }
}
