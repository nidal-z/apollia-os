/**
 * What a consent card needs to know about a call that runs code, and the
 * answers a card offers when the request does not name them.
 *
 * The runtime decides which answers a call is offered and sends them with the
 * request. A card rebuilt from a stored message has no request to read, so it
 * falls back on the rule the runtime applies by default: a tool that runs code
 * is never offered "always" there.
 */
import type { ConsentAnswer } from "./ConsentAnswers.svelte";

/** The tools whose argument is code, as the runtime lists them. */
export const CODE_EXECUTOR_TOOLS: readonly string[] = ["bash_executor", "python_executor"];

/** Whether `toolName` runs code. */
export function isCodeExecutor(toolName: string): boolean {
  return CODE_EXECUTOR_TOOLS.includes(toolName);
}

function stringField(input: unknown, key: string): string | null {
  if (typeof input !== "object" || input === null) return null;
  const value = (input as Record<string, unknown>)[key];
  return typeof value === "string" ? value : null;
}

/** The whole code a call of `toolName` runs, `null` for a tool that runs none. */
export function codeOf(toolName: string, input: unknown): string | null {
  if (!isCodeExecutor(toolName)) return null;
  const key = toolName === "python_executor" ? "code" : "command";
  return stringField(input, key) ?? "";
}

/** The folder a call names to run in, `null` when it names none. */
export function workingDirOf(input: unknown): string | null {
  const dir = stringField(input, "working_dir");
  return dir !== null && dir.trim().length > 0 ? dir : null;
}

/**
 * Whether `command` is a single simple command: no shell control that would
 * chain, pipe, redirect, substitute or background a second one. Mirrors the
 * check the runtime applies before it writes a rule on a command.
 */
export function isSingleSimpleCommand(command: string): boolean {
  const control = [";", "|", "&", "\n", "\r", "`", ">", "<", "(", ")", "{", "}"];
  return (
    !control.some((c) => command.includes(c)) &&
    !command.includes("$(") &&
    !command.includes("${")
  );
}

const KNOWN: readonly ConsentAnswer[] = ["once", "always", "refuse"];

/**
 * The answers a card offers: those the request names, in the card's order,
 * or, when it names none, all three for an ordinary tool and "once" and
 * "refuse" for a tool that runs code.
 */
export function offeredAnswers(
  toolName: string,
  named: readonly string[] | null | undefined,
): ConsentAnswer[] {
  if (named && named.length > 0) {
    return KNOWN.filter((answer) => named.includes(answer));
  }
  return isCodeExecutor(toolName) ? ["once", "refuse"] : [...KNOWN];
}
