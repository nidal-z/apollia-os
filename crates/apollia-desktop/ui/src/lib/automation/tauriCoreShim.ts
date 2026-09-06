/**
 * Dev-server stand-in for `@tauri-apps/api/core` (see vite.config.ts): the
 * real module, with `invoke` routed through the automation seam so a recipe's
 * stubInvoke step can answer an IPC call. The relative import bypasses the
 * alias, which only matches the bare specifier.
 */
import * as real from "../../../node_modules/@tauri-apps/api/core.js";
import type { InvokeArgs, InvokeOptions } from "../../../node_modules/@tauri-apps/api/core.js";
import { seam } from "./invokeSeam";

export * from "../../../node_modules/@tauri-apps/api/core.js";

// The seam takes `unknown` arguments so a stub can answer any command; the
// real function narrows them to Tauri's own types at the call.
seam.real = (cmd, args, options) =>
  real.invoke(cmd, args as InvokeArgs | undefined, options as InvokeOptions | undefined);

export function invoke<T>(cmd: string, args?: unknown, options?: unknown): Promise<T> {
  return seam.invoke(cmd, args, options) as Promise<T>;
}
