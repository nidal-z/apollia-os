// Dev-only automation harness: declarative step + report contract.
//
// This module is imported dynamically from App.svelte behind an
// `import.meta.env.DEV` guard, so it (and runner.ts) is tree-shaken out of any
// production build. No i18n, no user-facing strings: it is a dev tool.

import type { Route } from "$lib/stores/navigation";

/** A DOM target expressed as an exact or prefix `data-testid` match. */
export interface Target {
  /** Exact `[data-testid="testid"]`. */
  testid?: string;
  /** Prefix `[data-testid^="testidPrefix"]` for interpolated/non-unique ids. */
  testidPrefix?: string;
  /** Index among matches (defaults to 0); used for non-unique rows. */
  nth?: number;
}

export interface GotoStep {
  kind: "goto";
  route: Route;
}
export interface WaitForStep extends Target {
  kind: "waitFor";
  /** Poll deadline (defaults to 15000; pass ~120000 for chat/inference waits). */
  timeoutMs?: number;
}
export interface WaitGoneStep extends Target {
  kind: "waitGone";
  /** Poll until the target is absent/invisible. Use ~120000 for a chat turn to
   *  finish (e.g. the chat-stop-button disappearing). */
  timeoutMs?: number;
}
export interface ClickStep extends Target {
  kind: "click";
}
export interface FillStep extends Target {
  kind: "fill";
  text: string;
}
export interface SendChatStep {
  kind: "sendChat";
  text: string;
}
export interface ExpectStep extends Target {
  kind: "expect";
  /** When set, the target text must contain this substring. */
  contains?: string;
}
export interface CaptureTextStep extends Target {
  kind: "captureText";
  /** Key under which the captured text is recorded in the report. */
  as: string;
}
export interface ScreenshotStep {
  kind: "screenshot";
  label: string;
}
export interface SleepStep {
  kind: "sleep";
  ms: number;
}

export type Step =
  | GotoStep
  | WaitForStep
  | WaitGoneStep
  | ClickStep
  | FillStep
  | SendChatStep
  | ExpectStep
  | CaptureTextStep
  | ScreenshotStep
  | SleepStep;

export interface Script {
  name: string;
  /** Abort the run on the first failing step (default: continue). */
  stopOnError?: boolean;
  /** Marks a script that touches irreversible surfaces; refused unless the env
   *  flag APOLLIA_AUTOMATION_ALLOW_DESTRUCTIVE is set. */
  destructive?: boolean;
  steps: Step[];
}

export interface StepResult {
  index: number;
  kind: Step["kind"];
  ok: boolean;
  detail: string;
  tsMs: number;
}

export interface RunReport {
  script: string;
  startedAt: string;
  finishedAt: string;
  ok: boolean;
  steps: StepResult[];
  captures: Record<string, string>;
  screenshots: string[];
}

/** Payload returned by the `automation_script` Tauri command. */
export interface AutomationBoot {
  script: string;
  allowDestructive: boolean;
}
