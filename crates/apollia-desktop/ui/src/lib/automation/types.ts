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
  /** Chat session to open on arrival, set on `pendingChatSessionId` before the
   *  navigation (the order the command palette uses). An id no table holds is
   *  the dangling deep link the not-found screen exists for. Keep the palette
   *  clicks where a recipe already has them: this is for the ghost id, not a
   *  shortcut around the palette. */
  sessionId?: string;
}
export interface WaitForStep extends Target {
  kind: "waitFor";
  /** Poll deadline (defaults to 15000; pass ~120000 for chat/inference waits). */
  timeoutMs?: number;
  /** When set, the target must also read this fragment (a count, a status). */
  contains?: string;
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
  /**
   * `${HOME}` expands to the boot's `homeDir` (the seeded, throwaway home).
   * On an `input[type=file]` the text becomes the content of one text/plain
   * file named `automation-attachment.txt`, built in the webview.
   */
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
  /**
   * The shortcut modifier of the machine playing the book: Command on macOS,
   * Control elsewhere, matching what the application's dispatcher reads. Use
   * it for a product shortcut; use `meta` only to record a literal chord.
   */
  mod?: boolean;
  shift?: boolean;
  alt?: boolean;
  timeoutMs?: number;
}

/** Answer one Tauri command from the script instead of the backend.
 *
 *  Every webview call funnels through the `invoke` of `@tauri-apps/api/core`, the
 *  plugin-dialog pickers included (`plugin:dialog|open`, `plugin:dialog|save`),
 *  so one seam reaches every IPC-borne fault and every native picker: a list
 *  that rejects opens the error panel, a picker that resolves a path skips the
 *  OS dialog, `resolve: null` is a cancelled picker. Exactly one of `resolve`,
 *  `reject`, `patch` is given (the key must be present; its value may be null).
 *  Strings inside them expand `${HOME}`. The first stub whose `argsMatch` (each
 *  listed key deep-equal to the call's argument) accepts the call answers it;
 *  a stub without `argsMatch` accepts every call of its command. Everything
 *  not stubbed, the runner's own `automation_*` calls included, reaches the
 *  backend as before. The wrapper is installed by the first `stubInvoke` of a
 *  run and removed when the run ends, whatever ended it. A `patch` on
 *  `get_chat_session` also reaches `awaitTurn`'s own status poll: never leave
 *  one armed across a turn. */
export interface StubInvokeStep {
  kind: "stubInvoke";
  /** Tauri command name, e.g. `list_projects` or `plugin:dialog|open`. */
  command: string;
  /** Resolve with this value (JSON `null` is a legal value: a cancelled picker). */
  resolve?: unknown;
  /** Reject with this value; the UI classifies it (`.kind`, message text). */
  reject?: unknown;
  /** Let the real call run, then merge these fields over its result. */
  patch?: Record<string, unknown>;
  /** Fire once, then drop the stub (default: stays armed until cleared). */
  once?: boolean;
  /** Fire only when each key deep-equals the call's argument of that name. */
  argsMatch?: Record<string, unknown>;
}
/** Drop the stubs of one command, or every stub when `command` is absent. The
 *  invoke wrapper stays installed until the run ends; a cleared command simply
 *  passes through again. */
export interface ClearStubsStep {
  kind: "clearStubs";
  command?: string;
}
/** Resize the app window to a logical size, lifting the `tauri.conf.json`
 *  minimum (900 x 600) so the narrow-layout anchors (sidebar drawer under
 *  768 px, panes-as-drawers under 1024 px) become reachable, and putting the
 *  minimum back once the requested size satisfies it. The runner waits 400 ms
 *  for the reflow and the matchMedia listeners, and restores 1280 x 800 when
 *  the run ends, whatever ended it. A narrow block must contain no boundary
 *  click: a hung step keeps the window narrow for the rest of the boot. */
export interface ResizeWindowStep {
  kind: "resizeWindow";
  width: number;
  height: number;
}
/**
 * Inject a fault the product cannot be asked for through IPC. `heartbeat-lost`
 * mutes the runtime heartbeat listeners and fires the watchdog, so the
 * disconnected banner shows; its retry button unmutes them.
 */
export interface FaultStep {
  kind: "fault";
  name: "heartbeat-lost";
}
/**
 * Emit a Tauri event from the webview, as the backend would. Reaches every
 * listener, this webview's included. `${HOME}` expands inside the payload.
 */
export interface EmitEventStep {
  kind: "emitEvent";
  event: string;
  payload?: unknown;
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
  | PressStep
  | StubInvokeStep
  | ClearStubsStep
  | ResizeWindowStep
  | FaultStep
  | EmitEventStep;

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
  /** The home the process runs under (the seeded, throwaway one in a recipe);
   *  substituted for `${HOME}` in `fill.text` and in `stubInvoke` values.
   *  Empty when no home resolves, in which case `${HOME}` is refused. */
  homeDir: string;
}
