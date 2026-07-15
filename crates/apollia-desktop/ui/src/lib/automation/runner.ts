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
  const inner = el.querySelector<HTMLInputElement | HTMLTextAreaElement>(
    "input, textarea, [contenteditable='true']",
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
  return invoke<string>("automation_capture", { label });
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
      const el = await waitForEl(sel.css, DEFAULT_TIMEOUT_MS, step.nth);
      el.scrollIntoView({ block: "center" });
      el.click();
      return `clicked ${sel.label}`;
    }
    case "fill": {
      const sel = selectorFor(step);
      const el = await waitForEl(sel.css, DEFAULT_TIMEOUT_MS, step.nth);
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
    case "sleep": {
      await sleep(step.ms);
      return `slept ${step.ms}ms`;
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
