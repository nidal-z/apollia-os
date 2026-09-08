// Dev-only automation harness: the IPC stub seam.
//
// Every Tauri call of the webview goes through the `invoke` of
// `@tauri-apps/api/core`, and the plugin-dialog pickers are ordinary calls on
// it (`plugin:dialog|open`, `plugin:dialog|save`). The webview's own
// `window.__TAURI_INTERNALS__.invoke` is read-only, so the dev server aliases
// that module to tauriCoreShim.ts, whose `invoke` reads `seam.invoke` at call
// time (invokeSeam.ts). Replacing that one function lets a script answer a
// command without the backend, which is how a recipe reaches the error and
// empty states no seed produces and the pickers no runner can otherwise
// answer. Imported by runner.ts only, which App.svelte loads behind
// `import.meta.env.DEV`, so none of this ships.
//
// The class takes the host object rather than importing the seam itself so it
// is unit-tested against a plain object under the node environment.

export type InvokeFn = (cmd: string, args?: unknown, options?: unknown) => Promise<unknown>;

/** The object carrying `invoke`: the automation seam in the webview. */
export interface InvokeHost {
  invoke: InvokeFn;
}

export type StubMode = "resolve" | "reject" | "patch";

export interface Stub {
  command: string;
  mode: StubMode;
  /** The resolved value, the rejection payload, or the fields merged over the
   *  real result, by `mode`. */
  value: unknown;
  /** Dropped after its first hit. */
  once: boolean;
  /** When set, the stub answers only a call whose argument object carries
   *  every listed key with a deep-equal value. */
  argsMatch?: Record<string, unknown>;
}

/** What a recipe may name without knowing where the run put it. */
export interface RunTokens {
  /** The boot's home, for `${HOME}`. */
  home: string;
  /** An interpreter that starts, for `${PYTHON}`; empty when none answered. */
  python: string;
}

const TOKEN_NAMES: Array<[string, keyof RunTokens]> = [
  ["${HOME}", "home"],
  ["${PYTHON}", "python"],
];

/** Replace every run token in the strings of `value`, recursing through arrays
 *  and plain objects; other values pass through untouched. Refuses a token
 *  whose value is empty, so a boot that resolved nothing cannot expand it away
 *  and quietly point a picker at `/.apollia` or declare a server with no
 *  command. */
export function expandTokens<T>(value: T, tokens: RunTokens): T {
  if (typeof value === "string") {
    let out: string = value;
    for (const [token, key] of TOKEN_NAMES) {
      if (!out.includes(token)) continue;
      const replacement = tokens[key];
      if (!replacement) {
        throw new Error(`${token} used but the boot resolved none`);
      }
      out = out.split(token).join(replacement);
    }
    return out as T;
  }
  if (Array.isArray(value)) {
    return value.map((v: unknown) => expandTokens(v, tokens)) as T;
  }
  if (value !== null && typeof value === "object") {
    const out: Record<string, unknown> = {};
    for (const [k, v] of Object.entries(value as Record<string, unknown>)) {
      out[k] = expandTokens(v, tokens);
    }
    return out as T;
  }
  return value;
}

function deepEqual(a: unknown, b: unknown): boolean {
  if (a === b) return true;
  if (a === null || b === null || typeof a !== "object" || typeof b !== "object") return false;
  if (Array.isArray(a) !== Array.isArray(b)) return false;
  const ra = a as Record<string, unknown>;
  const rb = b as Record<string, unknown>;
  const ka = Object.keys(ra);
  if (ka.length !== Object.keys(rb).length) return false;
  return ka.every((k) => deepEqual(ra[k], rb[k]));
}

/** True when every key of `expected` is deep-equal to the same key of `args`. */
export function argsMatch(args: unknown, expected: Record<string, unknown>): boolean {
  const keys = Object.keys(expected);
  if (keys.length === 0) return true;
  if (args === null || typeof args !== "object") return false;
  const actual = args as Record<string, unknown>;
  return keys.every((k) => deepEqual(actual[k], expected[k]));
}

function mergePatch(base: unknown, patch: unknown): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  if (base !== null && typeof base === "object") Object.assign(out, base);
  if (patch !== null && typeof patch === "object") Object.assign(out, patch);
  return out;
}

/** Reads the webview's invoke host; `null` outside Tauri (unit tests, a browser). */

/** The stubs of one run. The wrapper is installed on the first `add` and
 *  removed by `restore`; between the two, every call the table does not
 *  answer reaches the original `invoke` untouched. */
export class InvokeStubs {
  private readonly stubs = new Map<string, Stub[]>();
  private original: InvokeFn | null = null;

  constructor(private readonly host: InvokeHost) {}

  get installed(): boolean {
    return this.original !== null;
  }

  add(stub: Stub): void {
    if (this.original === null) {
      const original = this.host.invoke;
      const wrapper: InvokeFn = (cmd, args, options) => this.dispatch(original, cmd, args, options);
      // The internals object may refuse a plain assignment (a frozen or
      // read-only property); say so with the property's descriptor rather
      // than leaving a wrapper that never took.
      try {
        this.host.invoke = wrapper;
      } catch (e) {
        throw new Error(`cannot replace the seam's invoke: ${String(e)}`);
      }
      if (this.host.invoke !== wrapper) {
        const desc = Object.getOwnPropertyDescriptor(this.host, "invoke");
        throw new Error(
          `the seam's invoke did not take the wrapper (writable=${desc?.writable}, configurable=${desc?.configurable}, frozen=${Object.isFrozen(this.host)})`,
        );
      }
      this.original = original;
    }
    const list = this.stubs.get(stub.command) ?? [];
    list.push(stub);
    this.stubs.set(stub.command, list);
  }

  /** Drop the stubs of one command, or all of them; returns how many went.
   *  The wrapper stays installed: a cleared command passes through again. */
  clear(command?: string): number {
    if (command !== undefined) {
      const n = this.stubs.get(command)?.length ?? 0;
      this.stubs.delete(command);
      return n;
    }
    let n = 0;
    for (const list of this.stubs.values()) n += list.length;
    this.stubs.clear();
    return n;
  }

  /** Put the original `invoke` back and forget every stub. Idempotent. */
  restore(): void {
    if (this.original !== null) {
      try {
        this.host.invoke = this.original;
      } catch {
        // Nothing to do: the wrapper never took either.
      }
      this.original = null;
    }
    this.stubs.clear();
  }

  private dispatch(
    original: InvokeFn,
    cmd: string,
    args: unknown,
    options: unknown,
  ): Promise<unknown> {
    const list = this.stubs.get(cmd);
    const idx = list
      ? list.findIndex((s) => s.argsMatch === undefined || argsMatch(args, s.argsMatch))
      : -1;
    if (!list || idx < 0) return original(cmd, args, options);
    const stub = list[idx];
    if (stub.once) {
      list.splice(idx, 1);
      if (list.length === 0) this.stubs.delete(cmd);
    }
    switch (stub.mode) {
      case "resolve":
        return Promise.resolve(stub.value);
      case "reject":
        return Promise.reject(stub.value);
      case "patch":
        return original(cmd, args, options).then((result) => mergePatch(result, stub.value));
    }
  }
}
