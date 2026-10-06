import { describe, test, expect, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));

import { get } from "svelte/store";
import {
  EMPTY_BAR,
  SERVER_TOGGLE_SHORTCUT,
  STATUS_LABELS,
  copyAddress,
  loadAsk,
  loadModelTestId,
  loadableModels,
  nextSwitchPosition,
  reachableAddress,
  readServer,
  serverStatus,
  startFailure,
  switchServer,
  type LocalServerIpc,
  type ServerBarState,
} from "./localServer";
import { isMacPlatform } from "$lib/navigation/shortcuts";
import type { LocalServerRefusal, LocalServerView } from "$lib/ipc/localServer";
import type { ModelTarget, TargetGroups } from "$lib/chat/chatTargets";
import type { LoadedModelsResponse } from "$lib/types";
import { askModelLoad, modelLoadAsk } from "$lib/stores/modelResidency";
import {
  dispatchShortcutEvent,
  registerShortcutHandler,
} from "$lib/navigation/shortcutDispatcher";

const PORT = 7772;
const ADDRESS = `http://127.0.0.1:${PORT}`;

function view(running: boolean, lastError: string | null = null): LocalServerView {
  return {
    running,
    port: PORT,
    serveOnLan: false,
    address: ADDRESS,
    lastError,
    activeTokens: 0,
    imposedByPolicy: { serveOnLan: false, requireAuth: false, allowRequestMcp: false },
  };
}

/**
 * The runtime behind the three commands, held in memory. `taken` makes a
 * start answer what the shell rejects with when another program holds the
 * port; `broken` makes every call fail the way a stopped runtime does.
 */
function fakeRuntime(opts: { running?: boolean; taken?: boolean; broken?: boolean } = {}) {
  const state = { running: opts.running ?? false, lastError: null as string | null };
  const ipc: LocalServerIpc = {
    read: vi.fn(async () => {
      if (opts.broken) throw new Error("the Apollia service does not answer");
      return view(state.running, state.lastError);
    }),
    start: vi.fn(async () => {
      if (opts.broken) throw new Error("the Apollia service does not answer");
      if (opts.taken) {
        state.lastError = `port ${PORT} is already in use by another program`;
        const refusal: LocalServerRefusal = { kind: "port_in_use", message: state.lastError };
        throw refusal;
      }
      state.running = true;
      state.lastError = null;
      return view(true);
    }),
    stop: vi.fn(async () => {
      if (opts.broken) throw new Error("the Apollia service does not answer");
      state.running = false;
      return view(false, state.lastError);
    }),
  };
  return { ipc, state };
}

function model(route: string, path: string | null, source: "local" | "server" = "local"): ModelTarget {
  return {
    kind: "model",
    route,
    label: route,
    source,
    format: source === "local" ? "gguf" : null,
    server: source === "server" ? "lab" : null,
    parameters: null,
    path,
  };
}

function loaded(paths: string[]): LoadedModelsResponse {
  return {
    engineAvailable: true,
    maxLoaded: null,
    models: paths.map((modelPath) => ({
      engine: "llama-server",
      kind: "chat",
      model: modelPath,
      modelPath,
      port: 41001,
      nCtx: 4096,
      nGpuLayers: 0,
      loadedAt: "2026-10-01T09:00:00+00:00",
      lastUsedAt: "2026-10-01T09:00:00+00:00",
      activeRequests: 0,
    })),
  } as LoadedModelsResponse;
}

describe("the bar of the local server - state and switch", () => {
  test("a stopped server reads Stopped, and its switch starts it", async () => {
    // GIVEN a runtime whose server is stopped, read once
    const { ipc, state } = fakeRuntime();
    const before = await readServer(ipc);
    expect(serverStatus(before)).toBe("stopped");
    expect(STATUS_LABELS[serverStatus(before)]).toBe("dev.developer.server.status_stopped");

    // WHEN the switch is turned on
    const after = await switchServer(ipc, before, nextSwitchPosition(before));

    // THEN the start route was called, and the bar reads Running
    expect(ipc.start).toHaveBeenCalledTimes(1);
    expect(state.running).toBe(true);
    expect(serverStatus(after)).toBe("running");
    expect(STATUS_LABELS[serverStatus(after)]).toBe("dev.developer.server.status_running");
    expect(after.failure).toBeNull();
  });

  test("a running server reads Running, and its switch stops it", async () => {
    // GIVEN a runtime whose server runs, read once
    const { ipc, state } = fakeRuntime({ running: true });
    const before = await readServer(ipc);
    expect(serverStatus(before)).toBe("running");

    // WHEN the switch is turned to its other position
    const after = await switchServer(ipc, before, nextSwitchPosition(before));

    // THEN the stop route was called, never the start, and the bar reads Stopped
    expect(ipc.stop).toHaveBeenCalledTimes(1);
    expect(ipc.start).not.toHaveBeenCalled();
    expect(state.running).toBe(false);
    expect(serverStatus(after)).toBe("stopped");
  });

  test("a start on a taken port leaves the server stopped and names the port", async () => {
    // GIVEN a runtime whose port is held by another program
    const { ipc } = fakeRuntime({ taken: true });
    const before = await readServer(ipc);

    // WHEN the switch is turned on
    const after = await switchServer(ipc, before, true);

    // THEN the bar reads an error, with the port in force and no address
    expect(serverStatus(after)).toBe("error");
    expect(after.failure).toEqual({ kind: "port_in_use", port: PORT });
    expect(after.view?.running).toBe(false);
    expect(reachableAddress(after)).toBeNull();
  });

  test("a start that succeeds clears the error of the one before", async () => {
    // GIVEN a bar left in error by a taken port
    const taken = fakeRuntime({ taken: true });
    const failed = await switchServer(taken.ipc, await readServer(taken.ipc), true);
    expect(failed.failure?.kind).toBe("port_in_use");

    // WHEN the port is free and the switch is turned on again
    const free = fakeRuntime();
    const after = await switchServer(free.ipc, failed, true);

    // THEN the error is gone
    expect(after.failure).toBeNull();
    expect(serverStatus(after)).toBe("running");
  });

  test("a rejection that is not a taken port is shown in the runtime's words", () => {
    // GIVEN three rejections: a typed one of another kind, an Error, a string
    const typed: LocalServerRefusal = { kind: "other", message: "failed to listen on port 7772" };

    // WHEN each becomes the failure of the bar
    const fromTyped = startFailure(typed, PORT);
    const fromError = startFailure(new Error("the Apollia service does not answer"), PORT);
    const fromString = startFailure("request failed", PORT);

    // THEN none is named a taken port, and each keeps its words
    expect(fromTyped).toEqual({ kind: "other", message: "failed to listen on port 7772" });
    expect(fromError).toEqual({ kind: "other", message: "the Apollia service does not answer" });
    expect(fromString).toEqual({ kind: "other", message: "request failed" });
  });

  test("a runtime that does not answer leaves the switch unknown and says why", async () => {
    // GIVEN a runtime that answers nothing
    const { ipc } = fakeRuntime({ broken: true });

    // WHEN the bar reads the server for the first time
    const bar = await readServer(ipc);

    // THEN nothing is known of the server, and the reason is kept
    expect(bar.view).toBeNull();
    expect(serverStatus(bar)).toBe("error");
    expect(bar.failure).toEqual({
      kind: "unreadable",
      message: "the Apollia service does not answer",
    });
    expect(serverStatus(EMPTY_BAR)).toBe("unknown");
  });

  test("a stop that fails keeps the server as it was known", async () => {
    // GIVEN a bar that knows a running server, and a runtime gone since
    const known: ServerBarState = { view: view(true), failure: null };
    const { ipc } = fakeRuntime({ broken: true });

    // WHEN the switch is turned off
    const after = await switchServer(ipc, known, false);

    // THEN the server is still shown running, with the reason of the failure
    expect(after.view?.running).toBe(true);
    expect(after.failure?.kind).toBe("other");
  });
});

describe("the bar of the local server - shortcut", () => {
  test("the chord of the catalogue runs the handler the bar registers", () => {
    // GIVEN the handler of the switch, registered as the bar does
    const handler = vi.fn();
    const unregister = registerShortcutHandler({ id: SERVER_TOGGLE_SHORTCUT, handler });
    const mac = isMacPlatform();
    const event = {
      key: ".",
      ctrlKey: !mac,
      metaKey: mac,
      altKey: false,
      shiftKey: false,
      target: null,
      preventDefault: vi.fn(),
    } as unknown as KeyboardEvent;

    // WHEN the chord is pressed (Cmd and period on macOS, Ctrl and period elsewhere)
    dispatchShortcutEvent(event);
    unregister();
    dispatchShortcutEvent(event);

    // THEN the handler ran once, and no more once the bar is gone
    expect(handler).toHaveBeenCalledTimes(1);
  });
});

describe("the bar of the local server - address", () => {
  test("a running server shows the address of the state route, and copies it", async () => {
    // GIVEN a bar on a running server
    const { ipc } = fakeRuntime({ running: true });
    const bar = await readServer(ipc);
    const write = vi.fn(async () => undefined);

    // WHEN the address is copied
    const done = await copyAddress(bar, write);

    // THEN the clipboard received the address the runtime reports
    expect(reachableAddress(bar)).toBe(ADDRESS);
    expect(done).toBe(true);
    expect(write).toHaveBeenCalledWith(ADDRESS);
  });

  test("a stopped server has no address to copy", async () => {
    // GIVEN a bar on a stopped server
    const { ipc } = fakeRuntime();
    const bar = await readServer(ipc);
    const write = vi.fn(async () => undefined);

    // WHEN a copy is asked
    const done = await copyAddress(bar, write);

    // THEN nothing reached the clipboard
    expect(reachableAddress(bar)).toBeNull();
    expect(done).toBe(false);
    expect(write).not.toHaveBeenCalled();
  });

  test("a clipboard that refuses is reported, not thrown", async () => {
    // GIVEN a bar on a running server and a clipboard that refuses
    const bar: ServerBarState = { view: view(true), failure: null };
    const write = vi.fn(async () => {
      throw new Error("denied");
    });

    // WHEN the address is copied
    const done = await copyAddress(bar, write);

    // THEN the copy is reported as not done
    expect(done).toBe(false);
  });
});

describe("the bar of the local server - load a model", () => {
  const groups: TargetGroups = {
    agents: [],
    models: [
      model("qwen/qwen3-27b", "/models/qwen.gguf"),
      model("mistral/ministral-8b", "/models/ministral.gguf"),
      model("gpt-lab", null, "server"),
    ],
  };

  test("the picker offers the models of this computer that are not in memory", () => {
    // GIVEN two local models, one of them resident, and a model of a server
    const resident = loaded(["/models/qwen.gguf"]);

    // WHEN the picker lists what can be loaded
    const offered = loadableModels(groups, resident).map((m) => m.route);

    // THEN only the local model out of memory is offered
    expect(offered).toEqual(["mistral/ministral-8b"]);
  });

  test("choosing a model opens the loading window on it", () => {
    // GIVEN a model offered by the picker, and the loading window closed
    modelLoadAsk.set(null);
    const [chosen] = loadableModels(groups, loaded([]));
    const onloaded = vi.fn();

    // WHEN it is chosen
    askModelLoad(loadAsk(chosen, onloaded));

    // THEN the loading window is asked to open on that model, in its format
    const ask = get(modelLoadAsk);
    expect(ask?.modelId).toBe("qwen/qwen3-27b");
    expect(ask?.format).toBe("gguf");
    ask?.onloaded?.();
    expect(onloaded).toHaveBeenCalledTimes(1);
    modelLoadAsk.set(null);
  });

  test("the test id of a model survives the characters of its name", () => {
    // GIVEN a model whose route carries a slash and a dot
    const target = model("qwen/qwen3.8-27b", "/models/q.gguf");

    // WHEN its test id is built
    const testid = loadModelTestId(target);

    // THEN it is one stable token
    expect(testid).toBe("developer-load-model-qwen_qwen3_8-27b");
  });
});
