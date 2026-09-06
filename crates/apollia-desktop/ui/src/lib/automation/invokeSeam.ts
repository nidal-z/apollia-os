/**
 * The mutable seam the automation runner stubs.
 *
 * `window.__TAURI_INTERNALS__.invoke` is a read-only property in the Tauri 2
 * webview (measured: "Attempted to assign to readonly property"), so the seam
 * sits one level up: in the dev server, Vite aliases `@tauri-apps/api/core` to
 * `tauriCoreShim.ts`, whose `invoke` reads `seam.invoke` at call time. Every
 * app module and every plugin that imports the bare specifier goes through
 * it; production builds keep the real module and never load this file.
 */
export type InvokeFn = (cmd: string, args?: unknown, options?: unknown) => Promise<unknown>;

export interface InvokeSeam {
  /** The real `invoke`, set by the shim when it loads; read at call time. */
  real: InvokeFn | null;
  /** What the shim's `invoke` calls; the runner's stubs wrap this. */
  invoke: InvokeFn;
}

// `invoke` delegates to `real` at call time rather than being replaced by it,
// so the shim may load after the runner installed its wrapper without
// overwriting it, and a wrapper installed before the shim still reaches the
// real function once it is there.
export const seam: InvokeSeam = {
  real: null,
  invoke: (cmd, args, options) =>
    seam.real
      ? seam.real(cmd, args, options)
      : Promise.reject(new Error("the invoke seam is not wired: the Vite alias did not load tauriCoreShim.ts")),
};
