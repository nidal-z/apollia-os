// Dev-only automation harness runner.
//
// Executes a declarative JSON Script (see types.ts) against the live DOM by
// injecting real gestures onto stable `data-testid` selectors, capturing the
// window at checkpoints (via the dev-only Rust `automation_capture` command),
// and persisting a run report. Loaded dynamically from App.svelte behind an
// `import.meta.env.DEV` guard, so it never ships in a production build.

import { invoke } from "@tauri-apps/api/core";
import type {
  RunReport,
  Script,
  Step,
  StepResult,
  Target,
} from "./types";

const DEFAULT_TIMEOUT_MS = 15_000;
const POLL_INTERVAL_MS = 150;

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function cssEscape(value: string): string {
  return value.replace(/["\\]/g, "\\$&");
}

interface ResolvedSelector {
  css: string;
  label: string;
}

function selectorFor(target: Target): ResolvedSelector {
  if (target.testid) {
    return { css: `[data-testid="${cssEscape(target.testid)}"]`, label: target.testid };
  }
  if (target.testidPrefix) {
    return {
      css: `[data-testid^="${cssEscape(target.testidPrefix)}"]`,
      label: `${target.testidPrefix}*`,
    };
  }
  throw new Error("step needs a testid or testidPrefix");
}

function resolveEl(css: string, nth?: number): HTMLElement | null {
  const els = document.querySelectorAll<HTMLElement>(css);
  if (els.length === 0) return null;
  // Negative nth indexes from the end (-1 = last), handy for the latest message.
  const idx = nth === undefined ? 0 : nth < 0 ? els.length + nth : nth;
  return els[idx] ?? null;
}

function isVisible(el: HTMLElement): boolean {
  const rect = el.getBoundingClientRect();
  if (rect.width === 0 && rect.height === 0) return false;
  const style = getComputedStyle(el);
  return style.visibility !== "hidden" && style.display !== "none";
}

async function waitForEl(css: string, timeoutMs: number, nth?: number): Promise<HTMLElement> {
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    const el = resolveEl(css, nth);
    if (el && isVisible(el)) return el;
    if (Date.now() >= deadline) throw new Error(`timeout (${timeoutMs}ms) waiting for ${css}`);
    await sleep(POLL_INTERVAL_MS);
  }
}

async function waitGoneEl(css: string, timeoutMs: number, nth?: number): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    const el = resolveEl(css, nth);
    if (!el || !isVisible(el)) return;
    if (Date.now() >= deadline) throw new Error(`timeout (${timeoutMs}ms) waiting for ${css} to disappear`);
    await sleep(POLL_INTERVAL_MS);
  }
}

// Svelte 5 two-way bindings only react to native input/change events; setting
// `.value` alone is silently ignored. This is the central caveat of the runner.
function fillEl(el: HTMLElement, text: string): void {
  if (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement) {
    el.focus();
    el.value = text;
    el.dispatchEvent(new Event("input", { bubbles: true }));
    el.dispatchEvent(new Event("change", { bubbles: true }));
    return;
  }
  if (el.isContentEditable) {
    el.focus();
    el.textContent = text;
    el.dispatchEvent(new InputEvent("input", { bubbles: true }));
    return;
  }
  // Some testids sit on a wrapper (e.g. chat-input); descend to the field.
  // Skip hidden/file inputs: the chat-input wrapper holds a hidden file input
  // (chat-attach-input) before the textarea, and `.value = text` on a file
  // input throws, which used to make every sendChat fail.
  const inner = el.querySelector<HTMLInputElement | HTMLTextAreaElement>(
    "textarea, input:not([type='file']):not([type='hidden']), [contenteditable='true']",
  );
  if (inner) {
    fillEl(inner, text);
    return;
  }
  throw new Error("fill target is not (and does not wrap) an input");
}

function truncate(text: string, max = 60): string {
  return text.length > max ? `${text.slice(0, max)}...` : text;
}

interface Overlay {
  update(current: number, total: number, kind: string): void;
  done(ok: boolean): void;
}

function mountOverlay(scriptName: string): Overlay {
  const box = document.createElement("div");
  box.setAttribute("data-automation-overlay", "");
  Object.assign(box.style, {
    position: "fixed",
    bottom: "12px",
    right: "12px",
    zIndex: "2147483647",
    padding: "8px 12px",
    borderRadius: "8px",
    background: "rgba(17,17,17,0.88)",
    color: "#e6e6e6",
    font: "12px/1.4 ui-monospace, SFMono-Regular, Menlo, monospace",
    pointerEvents: "none",
    maxWidth: "42ch",
    boxShadow: "0 4px 16px rgba(0,0,0,0.4)",
  } satisfies Partial<CSSStyleDeclaration>);
  box.textContent = `automation: ${scriptName}`;
  document.body.appendChild(box);
  return {
    update(current, total, kind) {
      box.textContent = `${scriptName}  ${current}/${total}  ${kind}`;
    },
    done(ok) {
      box.textContent = `${scriptName}  ${ok ? "done ✓" : "done (with failures) ✗"}`;
      box.style.background = ok ? "rgba(20,83,45,0.9)" : "rgba(127,29,29,0.9)";
    },
  };
}

async function captureWindow(label: string): Promise<string> {
  // Hide the automation HUD and any transient product toast so neither bleeds
  // into a screenshot destined for the operator-help documentation. The native
  // `screencapture` runs out-of-process, so wait two frames for the browser to
  // paint the hidden state before the capture is taken, then restore.
  const chrome = document.querySelectorAll<HTMLElement>(
    "[data-automation-overlay], [data-testid='toast-container']",
  );
  const restore: Array<[HTMLElement, string]> = [];
  for (const el of chrome) {
    restore.push([el, el.style.visibility]);
    el.style.visibility = "hidden";
  }
  if (restore.length > 0) {
    await new Promise((resolve) =>
      requestAnimationFrame(() => requestAnimationFrame(() => resolve(null))),
    );
  }
  try {
    return await invoke<string>("automation_capture", { label });
  } finally {
    for (const [el, prev] of restore) {
      el.style.visibility = prev;
    }
  }
}

async function runStep(
  step: Step,
  captures: Record<string, string>,
  screenshots: string[],
): Promise<string> {
  switch (step.kind) {
    case "goto": {
      const { navigateTo } = await import("$lib/stores/navigation");
      navigateTo(step.route);
      return `navigated to ${step.route}`;
    }
    case "waitFor": {
      const sel = selectorFor(step);
      await waitForEl(sel.css, step.timeoutMs ?? DEFAULT_TIMEOUT_MS, step.nth);
      return `present: ${sel.label}`;
    }
    case "waitGone": {
      const sel = selectorFor(step);
      await waitGoneEl(sel.css, step.timeoutMs ?? DEFAULT_TIMEOUT_MS, step.nth);
      return `gone: ${sel.label}`;
    }
    case "click": {
      const sel = selectorFor(step);
      const el = await waitForEl(sel.css, step.timeoutMs ?? DEFAULT_TIMEOUT_MS, step.nth);
      el.scrollIntoView({ block: "center" });
      el.click();
      return `clicked ${sel.label}`;
    }
    case "fill": {
      const sel = selectorFor(step);
      const el = await waitForEl(sel.css, step.timeoutMs ?? DEFAULT_TIMEOUT_MS, step.nth);
      fillEl(el, step.text);
      return `filled ${sel.label}`;
    }
    case "sendChat": {
      const input = await waitForEl(`[data-testid="chat-input"]`, DEFAULT_TIMEOUT_MS);
      fillEl(input, step.text);
      const btn = await waitForEl(`[data-testid="chat-send-button"]`, DEFAULT_TIMEOUT_MS);
      btn.click();
      return `sent chat: ${truncate(step.text)}`;
    }
    case "expect": {
      const sel = selectorFor(step);
      const el = resolveEl(sel.css, step.nth);
      if (!el) throw new Error(`expected ${sel.label} not found`);
      if (step.contains && !(el.textContent ?? "").includes(step.contains)) {
        throw new Error(`expected ${sel.label} to contain "${step.contains}"`);
      }
      return `ok: ${sel.label}`;
    }
    case "captureText": {
      const sel = selectorFor(step);
      const el = resolveEl(sel.css, step.nth);
      const text = (el?.textContent ?? "").trim();
      captures[step.as] = text;
      return `captured ${step.as} (${text.length} chars)`;
    }
    case "screenshot": {
      const path = await captureWindow(step.label);
      screenshots.push(path);
      return `screenshot ${step.label}`;
    }
    case "awaitTurn": {
      const timeout = step.timeoutMs ?? 180_000;
      const approve = step.approve !== false;
      const maxApprovals = step.maxApprovals ?? 25;
      // Both personas: builder renders ApprovalCard under chat-approval-inline,
      // operator renders OperatorApprovalCard (operator-approval-*) in the
      // reasoning stream. awaitTurn must detect and accept either.
      const cardCss = `[data-testid="chat-approval-inline"], [data-testid^="operator-approval-"]`;
      const acceptCss = `[data-testid^="approval-accept-"], [data-testid^="operator-approval-accept-"]`;
      const askCss = `[data-testid="chat-ask-user-inline"]`;
      const planCss = `[data-testid="chat-plan-review"]`;
      // DOM activity indicators (any visible => the turn is still moving).
      const busyCss = [
        `[data-testid="chat-stop-button"]`,
        `[data-testid="chat-message-streaming"]`,
        `[data-testid="chat-tool-executing"]`,
        `[data-testid="chat-live-reasoning"]`,
        `[data-testid="chat-agent-loading"]`,
      ];
      const anyVisible = (list: string[]): boolean =>
        list.some((c) => {
          const e = resolveEl(c);
          return !!e && isVisible(e);
        });
      // The backend session.status ("processing" vs "active") is the race-free
      // truth: it stays "processing" for the WHOLE turn, incl. the ReAct
      // inter-step gaps and HITL pauses where every DOM flag goes transiently
      // false (which used to make the next sendChat hit "busy on exchange").
      // Read it (fresh) only when the DOM looks idle, to confirm the turn ended.
      let sessionId: string | null = null;
      try {
        const [chatStore, svelteStore] = await Promise.all([
          import("$lib/stores/chat"),
          import("svelte/store"),
        ]);
        sessionId = svelteStore.get(chatStore.currentSession)?.id ?? null;
      } catch {
        // no store access: fall back to DOM-only completion
      }
      const startedAt = Date.now();
      const deadline = startedAt + timeout;
      const graceUntil = startedAt + 45_000;
      let approvals = 0;
      let started = false;
      let idle = 0;
      for (;;) {
        if (Date.now() >= deadline) {
          throw new Error(`awaitTurn timeout (${timeout}ms) after ${approvals} HITL approval(s)`);
        }
        const busyDom = anyVisible(busyCss);
        const cardEl = resolveEl(cardCss);
        const cardVisible = !!cardEl && isVisible(cardEl);
        if (busyDom || cardVisible) started = true;
        // Auto-accept HITL approval cards (handles 0..N sequential tool calls).
        if (approve && cardVisible) {
          const accept = resolveEl(acceptCss);
          if (accept && isVisible(accept)) {
            accept.scrollIntoView({ block: "center" });
            accept.click();
            approvals += 1;
            if (approvals > maxApprovals) {
              throw new Error(
                `awaitTurn exceeded ${maxApprovals} approvals (runaway turn; narrow the prompt scope)`,
              );
            }
            if (step.label) {
              try {
                screenshots.push(await captureWindow(`${step.label}-hitl-${approvals}`));
              } catch {
                // a capture failure must never abort the turn
              }
            }
            idle = 0;
            await sleep(500);
            continue;
          }
        }
        // Yield to the caller for states that need scripted input.
        const askEl = resolveEl(askCss);
        if (askEl && isVisible(askEl)) return `paused for ask_user (${approvals} approval(s))`;
        const planEl = resolveEl(planCss);
        if (planEl && isVisible(planEl)) return `paused for plan review (${approvals} approval(s))`;
        if (busyDom || cardVisible) {
          idle = 0;
          await sleep(POLL_INTERVAL_MS);
          continue;
        }
        // DOM looks idle: confirm the backend really finished (covers the gap).
        let backendActive = true;
        if (sessionId) {
          try {
            const detail = await invoke<{ status: string }>("get_chat_session", {
              sessionId,
            });
            backendActive = detail.status === "active";
            if (detail.status === "processing") started = true;
          } catch {
            // transient IPC failure: don't settle on it, re-poll next tick
            backendActive = false;
          }
        }
        if (backendActive && (started || Date.now() > graceUntil)) {
          idle += 1;
          if (idle >= 2) return `turn settled (${approvals} HITL approval(s))`;
        } else {
          idle = 0;
        }
        await sleep(POLL_INTERVAL_MS);
      }
    }
    case "sleep": {
      await sleep(step.ms);
      return `slept ${step.ms}ms`;
    }
    case "setChecked": {
      const sel = selectorFor(step);
      const el = await waitForEl(sel.css, DEFAULT_TIMEOUT_MS, step.nth);
      // Native checkbox/radio, directly or wrapped.
      const box =
        el instanceof HTMLInputElement
          ? el
          : el.querySelector<HTMLInputElement>("input[type='checkbox'], input[type='radio']");
      if (box) {
        if (box.checked !== step.checked) {
          // Toggle via the wrapping <label> when present: a single spec-correct
          // activation that fires the native change event Svelte 5's bind:checked
          // listens to. Setting .checked + a synthetic event does NOT sync the
          // binding, and clicking the input directly double-toggles via the label.
          const label = box.closest("label");
          (label ?? box).click();
        }
        return `setChecked ${sel.label} = ${step.checked}`;
      }
      // The app's canonical Checkbox/Toggle render as <button role="checkbox"|
      // "switch" aria-checked> with no native input. Read the ARIA state and
      // click only on a mismatch, so the step stays idempotent.
      const aria =
        el.getAttribute("role") === "checkbox" || el.getAttribute("role") === "switch"
          ? el
          : el.querySelector<HTMLElement>("[role='checkbox'], [role='switch']");
      if (!aria) {
        throw new Error("setChecked target is not a checkbox, switch, or wrapper of one");
      }
      if ((aria.getAttribute("aria-checked") === "true") !== step.checked) {
        aria.click();
      }
      return `setChecked ${sel.label} = ${step.checked}`;
    }
    case "selectOption": {
      const sel = selectorFor(step);
      const el = await waitForEl(sel.css, step.timeoutMs ?? DEFAULT_TIMEOUT_MS, step.nth);
      const select =
        el instanceof HTMLSelectElement ? el : el.querySelector<HTMLSelectElement>("select");
      if (!select) throw new Error("selectOption target is not (and does not wrap) a <select>");
      const opts = Array.from(select.options);
      let idx = -1;
      if (step.value !== undefined) {
        idx = opts.findIndex((o) => o.value === step.value);
      } else if (step.labelText !== undefined) {
        idx = opts.findIndex((o) => (o.textContent ?? "").trim() === step.labelText);
      } else if (step.index !== undefined) {
        idx = step.index;
      }
      if (idx < 0 || idx >= opts.length) {
        throw new Error(
          `selectOption: no option matching ${JSON.stringify(step.value ?? step.labelText ?? step.index)}`,
        );
      }
      select.focus();
      select.selectedIndex = idx;
      select.dispatchEvent(new Event("input", { bubbles: true }));
      select.dispatchEvent(new Event("change", { bubbles: true }));
      return `selectOption ${sel.label} = ${opts[idx].value}`;
    }
    case "press": {
      let target: HTMLElement;
      if (step.testid || step.testidPrefix) {
        const sel = selectorFor(step);
        target = await waitForEl(sel.css, step.timeoutMs ?? DEFAULT_TIMEOUT_MS, step.nth);
        target.focus();
      } else {
        target = (document.activeElement as HTMLElement | null) ?? document.body;
      }
      const init: KeyboardEventInit = {
        key: step.key,
        code: keyToCode(step.key),
        bubbles: true,
        cancelable: true,
        metaKey: step.meta === true,
        ctrlKey: step.ctrl === true,
        shiftKey: step.shift === true,
        altKey: step.alt === true,
      };
      target.dispatchEvent(new KeyboardEvent("keydown", init));
      target.dispatchEvent(new KeyboardEvent("keypress", init));
      target.dispatchEvent(new KeyboardEvent("keyup", init));
      const mods = `${step.meta ? "Meta+" : ""}${step.ctrl ? "Ctrl+" : ""}${step.shift ? "Shift+" : ""}${step.alt ? "Alt+" : ""}`;
      return `pressed ${mods}${step.key}`;
    }
  }
}

// Best-effort `KeyboardEvent.code` from a `key`. Handlers usually read `.key`;
// `.code` is provided for the few that gate on physical-key identity.
function keyToCode(key: string): string {
  if (key.length === 1) {
    const c = key.toUpperCase();
    if (c >= "A" && c <= "Z") return `Key${c}`;
    if (c >= "0" && c <= "9") return `Digit${c}`;
  }
  return key;
}

// Persisted UI-state localStorage keys leak across runs (WKWebView localStorage
// lives under the real ~/Library, not the swapped seed HOME), which makes
// state-dependent gestures non-deterministic: a stale
// `apollia.quickpicker.expanded` collapses the accordion the chat script asserts
// on, a stale `apollia.delete_automation.skip` bypasses the delete dialog. Clear
// the known non-deterministic UI-state keys so each component falls back to its
// default. Deliberately does NOT touch the MCP disclaimer key: removing it would
// re-trigger a blocking modal.
function resetDeterministicUiState(): void {
  if (typeof localStorage === "undefined") return;
  const keys = [
    "apollia.quickpicker.expanded",
    "apollia.delete_automation.skip",
    "apollia.next_steps.dismissed",
    "apollia.next_steps.feedback",
    "apollia.ui.sidebar",
  ];
  for (const key of keys) {
    try {
      localStorage.removeItem(key);
    } catch {
      // Private mode / quota - ignore.
    }
  }
}

export async function runAutomation(scriptJson: string, allowDestructive: boolean): Promise<void> {
  let script: Script;
  try {
    script = JSON.parse(scriptJson) as Script;
  } catch (e) {
    console.error("[automation] invalid script JSON", e);
    return;
  }

  if (script.destructive && !allowDestructive) {
    console.warn(
      `[automation] script "${script.name}" is marked destructive; set ` +
        "APOLLIA_AUTOMATION_ALLOW_DESTRUCTIVE to run it. Skipping.",
    );
    return;
  }

  resetDeterministicUiState();

  console.info(`[automation] running "${script.name}" (${script.steps.length} steps)`);
  const overlay = mountOverlay(script.name);
  const startedAt = new Date().toISOString();
  const steps: StepResult[] = [];
  const captures: Record<string, string> = {};
  const screenshots: string[] = [];
  let ok = true;

  for (let i = 0; i < script.steps.length; i++) {
    const step = script.steps[i];
    overlay.update(i + 1, script.steps.length, step.kind);
    const t0 = performance.now();
    let stepOk = true;
    let detail = "";
    try {
      detail = await runStep(step, captures, screenshots);
    } catch (e) {
      stepOk = false;
      ok = false;
      detail = e instanceof Error ? e.message : String(e);
      console.error(`[automation] step ${i + 1} (${step.kind}) failed: ${detail}`);
      try {
        screenshots.push(await captureWindow(`fail-${i + 1}-${step.kind}`));
      } catch {
        // a failing capture must not mask the step failure
      }
    }
    steps.push({
      index: i,
      kind: step.kind,
      ok: stepOk,
      detail,
      tsMs: Math.round(performance.now() - t0),
    });
    if (!stepOk && script.stopOnError) break;
  }

  const report: RunReport = {
    script: script.name,
    startedAt,
    finishedAt: new Date().toISOString(),
    ok,
    steps,
    captures,
    screenshots,
  };
  try {
    const path = await invoke<string>("automation_finish", {
      reportJson: JSON.stringify(report, null, 2),
    });
    console.info(`[automation] report written: ${path}`);
  } catch (e) {
    console.error("[automation] failed to write report", e);
  }
  overlay.done(ok);
}
