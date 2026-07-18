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
  /** Wait deadline for the element (default 15000). Use a short value for
   *  data-dependent controls so an absent one fails fast, not after 15s. */
  timeoutMs?: number;
}
export interface FillStep extends Target {
  kind: "fill";
  text: string;
  /** Wait deadline for the element (default 15000). */
  timeoutMs?: number;
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
/** Drives a chat/agent turn to completion, robust to non-deterministic HITL.
 *  While the agent is busy it auto-accepts every approval card that appears
 *  (`chat-approval-inline` -> first `approval-accept-*`), handling 0..N calls.
 *  It yields (returns early) when a state needing script input appears
 *  (`chat-ask-user-inline`, `chat-plan-review`) so the caller can drive it,
 *  then a second `awaitTurn` finishes the turn. Settles when the agent is idle. */
export interface AwaitTurnStep {
  kind: "awaitTurn";
  /** Auto-accept HITL approval cards (default true). */
  approve?: boolean;
  /** Overall deadline for the turn to settle (default 180000). */
  timeoutMs?: number;
  /** When set, each accepted HITL card is screenshotted as `{label}-hitl-{n}`. */
  label?: string;
  /** Runaway guard: abort if more than this many approvals fire in one turn
   *  (default 25). A prompt that makes the agent read the whole repo would
   *  otherwise loop forever accepting cards. */
  maxApprovals?: number;
}
/** Deterministically set a checkbox to a desired state (no blind toggle).
 *  Works on a native `input[type=checkbox|radio]` (directly or via a wrapping
 *  label) and on the app's canonical Checkbox/Toggle, which render as
 *  `<button role="checkbox"|"switch" aria-checked>` with no native input. */
export interface SetCheckedStep extends Target {
  kind: "setChecked";
  checked: boolean;
}
/** Select an option in a native `<select>` (directly or via a wrapping element).
 *  Native selects react to a programmatic `selectedIndex` + `change` event, so
 *  unlike checkboxes this is a reliable one-shot. Match by exact option `value`,
 *  by exact visible `labelText`, or by `index`. Fires `input`+`change` so Svelte
 *  `bind:value` picks it up. */
export interface SelectOptionStep extends Target {
  kind: "selectOption";
  /** Exact `<option value>` to select. */
  value?: string;
  /** Or exact visible option text. */
  labelText?: string;
  /** Or option position (0-based). */
  index?: number;
  timeoutMs?: number;
}
/** Dispatch a keyboard event (keydown/keypress/keyup) with optional modifiers.
 *  Targets the given testid (focused first) or, when none is given, the active
 *  element. Reaches window/document-level shortcut listeners via bubbling.
 *  Note: synthetic events are `isTrusted=false`; handlers that gate on that will
 *  not fire (keep those as open-and-screenshot boundaries instead). */
export interface PressStep extends Target {
  kind: "press";
  /** `KeyboardEvent.key`, e.g. "Enter", "Escape", "k", "ArrowDown". */
  key: string;
  meta?: boolean;
  ctrl?: boolean;
  shift?: boolean;
  alt?: boolean;
  timeoutMs?: number;
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
  | SleepStep
  | AwaitTurnStep
  | SetCheckedStep
  | SelectOptionStep
  | PressStep;

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
