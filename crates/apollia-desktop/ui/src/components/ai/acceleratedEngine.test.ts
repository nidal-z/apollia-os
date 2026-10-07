import { describe, test, expect, vi, beforeAll, beforeEach } from "vitest";
import { render as renderToString } from "svelte/server";
import { get } from "svelte/store";
import { locale, waitLocale } from "svelte-i18n";
import "$lib/i18n";

const ipc = vi.hoisted(() => ({
  invoke: vi.fn(async (_command: string, _args?: unknown): Promise<unknown> => null),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: ipc.invoke }));

import AcceleratedEngineCard from "./AcceleratedEngineCard.svelte";
import {
  cardView,
  createAcceleratedEngine,
  megabytes,
  releaseDay,
  remainingSeconds,
  type AcceleratedEngine,
  type AcceleratedEngineState,
  type CardView,
} from "./acceleratedEngine";
import { checkEngineUpdates, listEngines, type EnginesResponse } from "$lib/ipc/system";
import {
  dismissEngineInstallation,
  getEngineInstallation,
  importEngineFolder,
  startEngineInstallation,
  type EngineInstallation,
  type EngineRefusalCode,
} from "$lib/ipc/engineInstallation";
import { listLoadedModels, unloadLoadedModel } from "$lib/ipc/llm";
import { loadModel } from "$lib/ipc/llmLoad";
import { developerMode } from "$lib/stores/developerMode";
import type { LoadedModel } from "$lib/types";
import fr from "$lib/i18n/fr.json";

const copy = fr.ai.engine;
const MB = 1_000_000;
const FOLDER = "D:\\Apollia\\moteur-nvidia";

/** A catalogue sentence with its `{name}` placeholders filled. */
function fill(template: string, values: Record<string, string>): string {
  return template.replace(/\{(\w+)\}/g, (_, name: string) => values[name] ?? "");
}

/** The engines of a computer with an NVIDIA card and without the accelerated engine. */
function proposable(overrides: Partial<EnginesResponse> = {}): EnginesResponse {
  return {
    engines: [],
    channel: "preview",
    manifest_sequence: 12,
    updates: [
      {
        engine: "llama-server",
        variant: "cuda",
        installed_tag: null,
        latest_tag: "b7012",
        update_available: false,
        size: 412 * MB,
        latest_released: "2026-09-28",
        installed_released: null,
      },
    ],
    cuda_proposable: true,
    nvidia_card: "NVIDIA GeForce RTX 4070",
    dictation: null,
    ...overrides,
  };
}

/** The same computer once the engine is installed, the channel offering `latest`. */
function installed(latest: string, released: string | null = "2026-09-28"): EnginesResponse {
  return proposable({
    cuda_proposable: false,
    updates: [
      {
        engine: "llama-server",
        variant: "cuda",
        installed_tag: "b6870",
        latest_tag: latest,
        update_available: latest !== "b6870",
        size: 38 * MB,
        latest_released: released,
        installed_released: released === null ? null : "2026-09-12",
      },
    ],
  });
}

function refusal(
  kind: "install" | "update" | "import",
  code: EngineRefusalCode,
  archiveRequested = false,
): EngineInstallation {
  return {
    state: "failed",
    kind,
    code,
    error: `the store refused: ${code}`,
    archive_requested: archiveRequested,
  };
}

function loadedModel(overrides: Partial<LoadedModel> = {}): LoadedModel {
  return {
    engine: "llama-server",
    kind: "chat",
    model: "qwen3-8b-q4",
    modelPath: "/models/qwen3-8b-q4.gguf",
    port: 4100,
    nCtx: 8192,
    nGpuLayers: 0,
    loadedAt: "2026-10-07T08:00:00Z",
    lastUsedAt: "2026-10-07T08:05:00Z",
    activeRequests: 0,
    modelId: "qwen3-8b-q4",
    ...overrides,
  };
}

/**
 * A simulated runtime behind the IPC: the engines it lists, the states its
 * installation goes through read after read, and the models it holds.
 */
function runtime(options: {
  engines: EnginesResponse;
  /** Where the installation stands before any gesture. */
  initial?: EngineInstallation;
  /** What a start or an import answers, then what each read after it answers, in order. */
  installation?: EngineInstallation[];
  afterInstall?: EnginesResponse;
  loaded?: LoadedModel[];
  folder?: string | null;
}) {
  let engines = options.engines;
  const states = [...(options.installation ?? [])];
  let installation: EngineInstallation = options.initial ?? { state: "idle" };
  let started = false;
  const next = (): EngineInstallation => {
    installation = states.shift() ?? installation;
    if (installation.state === "installed" && options.afterInstall) engines = options.afterInstall;
    return installation;
  };
  ipc.invoke.mockImplementation(async (command: string) => {
    switch (command) {
      case "list_engines":
        return engines;
      case "check_engine_updates":
        return { channel: engines.channel, manifest_sequence: 12, updates: engines.updates };
      case "get_engine_installation":
        return started ? next() : installation;
      case "start_engine_installation":
      case "import_engine_folder":
        started = true;
        return next();
      case "dismiss_engine_installation":
        installation = { state: "idle" };
        return installation;
      case "list_loaded_models":
        return { engineAvailable: true, maxLoaded: null, models: options.loaded ?? [] };
      default:
        return null;
    }
  });
  const errors: unknown[] = [];
  let clock = 0;
  const engine = createAcceleratedEngine({
    listEngines,
    checkUpdates: checkEngineUpdates,
    installation: getEngineInstallation,
    start: startEngineInstallation,
    importFolder: importEngineFolder,
    dismiss: dismissEngineInstallation,
    pickFolder: async () => (options.folder === undefined ? FOLDER : options.folder),
    loaded: listLoadedModels,
    unload: unloadLoadedModel,
    load: loadModel,
    wait: async () => {
      clock += 60_000;
    },
    now: () => clock,
    onError: (err) => errors.push(err),
  });
  return { engine, errors };
}

/** The commands invoked so far, with their arguments, in order. */
function invoked(command: string): unknown[] {
  return ipc.invoke.mock.calls.filter((call) => call[0] === command).map((call) => call[1]);
}

/** Every card the screen showed while `gesture` ran, by its status. */
async function shown(engine: AcceleratedEngine, gesture: () => Promise<void>): Promise<CardView[]> {
  const views: CardView[] = [];
  const stop = engine.subscribe((state) => {
    const view = cardView(state);
    const last = views[views.length - 1];
    if (view && JSON.stringify(view) !== JSON.stringify(last)) views.push(view);
  });
  await gesture();
  stop();
  return views;
}

function card(view: CardView | null, developer = false): string {
  expect(view).not.toBeNull();
  developerMode.set(developer);
  return renderToString(AcceleratedEngineCard, {
    props: {
      view: view as CardView,
      busy: false,
      onaccelerate: () => {},
      onimport: () => {},
      onlater: () => {},
      oncancel: () => {},
      onclose: () => {},
      onreload: () => {},
    },
  }).body;
}

/** The text of the element carrying `data-testid="<testid>"`, markup and comments removed. */
function textOf(html: string, testid: string): string | null {
  const match = html.match(
    new RegExp(`<(\\w+)[^>]*data-testid="${testid}"[^>]*>([\\s\\S]*?)</\\1>`),
  );
  if (!match) return null;
  return match[2]
    .replace(/<!--[\s\S]*?-->/g, "")
    .replace(/<[^>]*>/g, "")
    .replace(/&#39;/g, "'")
    .replace(/\s+/g, " ")
    .trim();
}

function hasButton(html: string, testid: string): boolean {
  return html.includes(`data-testid="${testid}"`);
}

function stepStatus(html: string, testid: string): string | null {
  const match = html.match(new RegExp(`data-testid="${testid}" data-status="(\\w+)"`));
  return match ? match[1] : null;
}

beforeAll(async () => {
  locale.set("fr");
  await waitLocale("fr");
});

beforeEach(() => {
  ipc.invoke.mockReset();
});

describe("Accelerated engine - the proposal and its installation", () => {
  test("a computer with an NVIDIA card is offered the engine with its weight and a button", async () => {
    // GIVEN the engine route says the accelerated engine can be proposed, and
    //       the verified list states a 412 MB archive
    const { engine } = runtime({ engines: proposable() });

    // WHEN the AI screen reads it
    await engine.refresh();
    const html = card(cardView(get(engine)));

    // THEN the card names the card detected, the weight read in the verified
    //      list, and offers the one-click button and the import
    expect(textOf(html, "ai-engine-title")).toBe(copy.proposal.title);
    expect(html).toContain("NVIDIA GeForce RTX 4070");
    expect(textOf(html, "ai-engine-weight")).toBe(fill(copy.download_weight, { size: "412 Mo" }));
    expect(textOf(html, "ai-engine-start")).toBe(copy.action.accelerate);
    expect(textOf(html, "ai-engine-import")).toBe(copy.action.import);
    expect(textOf(html, "ai-engine-later")).toBe(copy.action.later);
    // AND the list was already verified, so it is not fetched again
    expect(invoked("check_engine_updates")).toHaveLength(0);
  });

  test("a computer that never verified the list asks for it once to read the weight", async () => {
    // GIVEN a proposable engine and no verified list yet
    const { engine } = runtime({ engines: proposable({ updates: [], manifest_sequence: null }) });

    // WHEN the AI screen reads it twice
    await engine.refresh();
    await engine.refresh();

    // THEN the list is asked for once, and the proposal stands without a weight
    expect(invoked("check_engine_updates")).toHaveLength(1);
    const html = card(cardView(get(engine)));
    expect(textOf(html, "ai-engine-weight")).toBe(copy.signature_promise);
  });

  test("the button shows the signature being verified, then the download, then the engine installed", async () => {
    // GIVEN a proposal, and a runtime that verifies, downloads, then installs
    const { engine, errors } = runtime({
      engines: proposable(),
      installation: [
        { state: "verifying", kind: "install" },
        { state: "downloading", kind: "install", received: 0, total: 412 * MB },
        { state: "downloading", kind: "install", received: 80 * MB, total: 412 * MB },
        { state: "installed", kind: "install" },
      ],
      afterInstall: installed("b6870"),
    });
    await engine.refresh();

    // WHEN the button is pressed and the installation followed to its end
    const views = await shown(engine, engine.accelerate);

    // THEN the verified path was asked for the accelerated engine
    expect(invoked("start_engine_installation")).toEqual([
      { engine: "llama-server", variant: "cuda" },
    ]);
    expect(errors).toEqual([]);
    const statuses = views.map((view) => view.status);
    expect(statuses.filter((s, i) => s !== statuses[i - 1])).toEqual([
      "proposal",
      "verifying",
      "downloading",
      "installed",
    ]);

    // AND the first step visible is the signature, the download waiting for it
    const verifying = card(views.find((v) => v.status === "verifying") ?? null);
    expect(textOf(verifying, "ai-engine-title")).toBe(copy.progress.install_title);
    expect(stepStatus(verifying, "ai-engine-step-signature")).toBe("running");
    expect(verifying).toContain(copy.step.signature);
    expect(stepStatus(verifying, "ai-engine-step-download")).toBe("pending");
    expect(verifying).toContain(fill(copy.step.download_pending, { size: "412 Mo" }));
    expect(hasButton(verifying, "ai-engine-cancel")).toBe(false);

    // AND the second is the progress of the download, with what is left
    const downloading = card(views.filter((v) => v.status === "downloading").pop() ?? null);
    expect(stepStatus(downloading, "ai-engine-step-signature")).toBe("done");
    expect(downloading).toContain(copy.step.signature_done);
    expect(stepStatus(downloading, "ai-engine-step-download")).toBe("running");
    expect(downloading).toContain(
      fill(copy.step.download_progress, { received: "80 Mo", total: "412 Mo" }),
    );
    expect(downloading).toContain(fill(copy.eta.minutes, { n: "4" }));
    expect(downloading).toMatch(/aria-valuenow="19\.4\d*"/);
    expect(textOf(downloading, "ai-engine-cancel")).toBe(copy.action.cancel);

    // AND on success the card says the next load uses the card, with no
    //     proposal left behind it
    const done = card(views[views.length - 1]);
    expect(textOf(done, "ai-engine-title")).toBe(copy.installed.title);
    expect(textOf(done, "ai-engine-body")).toBe(copy.installed.body);
    await engine.leave();
    expect(invoked("dismiss_engine_installation")).toHaveLength(1);
    expect(cardView(get(engine))).toBeNull();
  });

  test("a download is cancelled and the proposal comes back", async () => {
    // GIVEN a download in progress
    const { engine } = runtime({
      engines: proposable(),
      initial: { state: "downloading", kind: "install", received: 80 * MB, total: 412 * MB },
    });
    await engine.refresh();
    expect(cardView(get(engine))?.status).toBe("downloading");

    // WHEN it is cancelled
    await engine.cancel();

    // THEN the runtime is told, and the engine is proposed again
    expect(invoked("dismiss_engine_installation")).toHaveLength(1);
    expect(cardView(get(engine))?.status).toBe("proposal");
  });
});

describe("Accelerated engine - import from a folder", () => {
  test("the folder chosen is handed to the import, and a good folder ends installed", async () => {
    // GIVEN a proposal, and a runtime that verifies then installs a folder
    const { engine } = runtime({
      engines: proposable(),
      installation: [
        { state: "verifying", kind: "import" },
        { state: "installing", kind: "import" },
        { state: "installed", kind: "import" },
      ],
      afterInstall: installed("b6870"),
    });
    await engine.refresh();

    // WHEN a folder is chosen
    const views = await shown(engine, engine.importFromFolder);

    // THEN the import was called with that folder and went through its two steps
    expect(invoked("import_engine_folder")).toEqual([{ path: FOLDER }]);
    const verifying = card(views.find((v) => v.status === "verifying") ?? null);
    expect(textOf(verifying, "ai-engine-title")).toBe(copy.progress.import_title);
    expect(stepStatus(verifying, "ai-engine-step-signature")).toBe("running");
    expect(stepStatus(verifying, "ai-engine-step-installation")).toBe("pending");
    const installing = card(views.find((v) => v.status === "installing") ?? null);
    expect(stepStatus(installing, "ai-engine-step-signature")).toBe("done");
    expect(stepStatus(installing, "ai-engine-step-installation")).toBe("running");
    expect(views[views.length - 1].status).toBe("installed");
  });

  test("a picker closed without a folder imports nothing", async () => {
    // GIVEN a proposal and a picker the user closes
    const { engine } = runtime({ engines: proposable(), folder: null });
    await engine.refresh();

    // WHEN the import is asked for
    await engine.importFromFolder();

    // THEN nothing is sent and the proposal stays
    expect(invoked("import_engine_folder")).toHaveLength(0);
    expect(cardView(get(engine))?.status).toBe("proposal");
  });

  test.each([
    ["signature_missing", "signature_missing"],
    ["signature_invalid", "signature_invalid"],
    ["digest_mismatch", "archive_altered"],
  ] as const)(
    "a folder refused for %s shows its own readable error",
    async (code, reason) => {
      // GIVEN a runtime that refuses the folder for that cause
      const { engine } = runtime({
        engines: proposable(),
        installation: [{ state: "verifying", kind: "import" }, refusal("import", code)],
      });
      await engine.refresh();

      // WHEN the folder is imported
      await engine.importFromFolder();
      const html = card(cardView(get(engine)));

      // THEN the card names that cause and says nothing was installed
      expect(textOf(html, "ai-engine-title")).toBe(
        fill(copy.refusal.title.import, { cause: copy.refusal.cause[reason] }),
      );
      expect(textOf(html, "ai-engine-body")).toBe(
        `${copy.refusal.body[reason]} ${copy.refusal.nothing_installed}`,
      );
      // AND offers to close or to choose another folder
      expect(textOf(html, "ai-engine-close")).toBe(copy.action.close);
      expect(textOf(html, "ai-engine-other-folder")).toBe(copy.action.other_folder);
    },
  );

  test("the three import refusals read differently", () => {
    // GIVEN the three refusals of an import
    const htmls = (["signature_missing", "signature_invalid", "digest_mismatch"] as const).map(
      (code) =>
        card(
          cardView({
            engines: proposable(),
            installation: refusal("import", code),
            later: [],
            etaSeconds: null,
            loadedCount: 0,
            busy: false,
          }),
        ),
    );

    // WHEN their titles and texts are compared
    const titles = new Set(htmls.map((html) => textOf(html, "ai-engine-title")));
    const bodies = new Set(htmls.map((html) => textOf(html, "ai-engine-body")));

    // THEN no two are the same
    expect(titles.size).toBe(3);
    expect(bodies.size).toBe(3);
  });
});

describe("Accelerated engine - a signature refused at download", () => {
  test.each([
    ["signature_invalid", "list_altered"],
    ["signature_unknown_key", "unknown_key"],
    ["rollback", "older_version"],
  ] as const)(
    "%s says no file was downloaded and offers no way around",
    async (code, reason) => {
      // GIVEN a runtime that refuses the published list before any archive
      const { engine } = runtime({
        engines: proposable(),
        installation: [{ state: "verifying", kind: "install" }, refusal("install", code)],
      });
      await engine.refresh();

      // WHEN the button is pressed
      await engine.accelerate();
      const html = card(cardView(get(engine)));

      // THEN the card names the cause and says no file was downloaded
      expect(textOf(html, "ai-engine-title")).toBe(
        fill(copy.refusal.title.install, { cause: copy.refusal.cause[reason] }),
      );
      expect(textOf(html, "ai-engine-body")).toBe(
        `${copy.refusal.nothing_downloaded} ${copy.refusal.body[reason]}`,
      );
      expect(stepStatus(html, "ai-engine-step-signature")).toBe("refused");
      expect(stepStatus(html, "ai-engine-step-download")).toBe("pending");
      expect(html).toContain(copy.step.not_started);
      // AND the only gesture is to close: nothing starts the download anyway
      expect(textOf(html, "ai-engine-close")).toBe(copy.action.close);
      for (const testid of ["ai-engine-start", "ai-engine-import", "ai-engine-other-folder"]) {
        expect(hasButton(html, testid)).toBe(false);
      }
      expect(html.match(/<button/g)).toHaveLength(1);
      // AND the runtime's sentence stays out of the default view
      expect(hasButton(html, "ai-engine-detail")).toBe(false);
      expect(card(cardView(get(engine)), true)).toContain(`the store refused: ${code}`);

      // AND closing forgets the refusal
      await engine.close();
      expect(invoked("dismiss_engine_installation")).toHaveLength(1);
      expect(cardView(get(engine))?.status).toBe("proposal");
    },
  );

  test("an archive refused after its download does not claim that nothing was downloaded", () => {
    // GIVEN a refusal on the digest, once the archive was requested
    const view = cardView({
      engines: proposable(),
      installation: refusal("install", "digest_mismatch", true),
      later: [],
      etaSeconds: null,
      loadedCount: 0,
      busy: false,
    });

    // WHEN the card renders it
    const html = card(view);

    // THEN it says nothing was installed, never that nothing was downloaded
    expect(html).not.toContain(copy.refusal.nothing_downloaded);
    expect(html).toContain(copy.refusal.nothing_installed);
  });
});

describe("Accelerated engine - a new version on the followed channel", () => {
  test("an installed engine the channel has outgrown is offered its new version, without the channel", async () => {
    // GIVEN the route reports a newer build of the installed engine
    const { engine } = runtime({
      engines: installed("b7012"),
      installation: [
        { state: "verifying", kind: "update" },
        { state: "downloading", kind: "update", received: 7 * MB, total: 38 * MB },
        { state: "installed", kind: "update" },
      ],
      afterInstall: installed("b6870"),
    });
    await engine.refresh();
    const html = card(cardView(get(engine)));

    // THEN the card announces it, names both versions by their day, and the weight
    expect(textOf(html, "ai-engine-title")).toBe(copy.update.title);
    expect(textOf(html, "ai-engine-body")).toBe(
      fill(copy.update.body, {
        latest: fill(copy.version_day, { day: "28 septembre 2026" }),
        installed: fill(copy.version_day, { day: "12 septembre 2026" }),
      }),
    );
    expect(textOf(html, "ai-engine-weight")).toBe(fill(copy.download_weight, { size: "38 Mo" }));
    expect(textOf(html, "ai-engine-start")).toBe(copy.action.update);
    // AND neither the channel nor the build number is shown here
    expect(html).not.toMatch(/preview|stable|canal/i);
    expect(html).not.toContain("b7012");
    // AND the developer mode adds the build numbers after the days
    const developer = card(cardView(get(engine)), true);
    expect(developer).toContain("28 septembre 2026 (b7012)");
    expect(developer).toContain("12 septembre 2026 (b6870)");

    // WHEN the button is pressed
    const views = await shown(engine, engine.accelerate);

    // THEN the same verified path is started, and it reads as an update
    expect(invoked("start_engine_installation")).toEqual([
      { engine: "llama-server", variant: "cuda" },
    ]);
    const verifying = card(views.find((v) => v.status === "verifying") ?? null);
    expect(textOf(verifying, "ai-engine-title")).toBe(
      fill(copy.progress.update_title_version, {
        version: fill(copy.version_day, { day: "28 septembre 2026" }),
      }),
    );
    expect(stepStatus(verifying, "ai-engine-step-signature")).toBe("running");
    const done = card(views[views.length - 1]);
    expect(textOf(done, "ai-engine-title")).toBe(copy.installed.update_title);
  });

  test("a channel that states no day names the version by its tag", () => {
    // GIVEN an update whose list carries no release day
    const view = cardView({
      engines: installed("b7012", null),
      installation: { state: "idle" },
      later: [],
      etaSeconds: null,
      loadedCount: 0,
      busy: false,
    });

    // WHEN the card renders it
    const html = card(view);

    // THEN the tag names both versions
    expect(textOf(html, "ai-engine-body")).toBe(
      fill(copy.update.body, {
        latest: fill(copy.version_number, { tag: "b7012" }),
        installed: fill(copy.version_number, { tag: "b6870" }),
      }),
    );
  });

  test("a refusal during an update says the current engine stays in use", () => {
    // GIVEN an update whose published list was altered
    const view = cardView({
      engines: installed("b7012"),
      installation: refusal("update", "signature_invalid"),
      later: [],
      etaSeconds: null,
      loadedCount: 0,
      busy: false,
    });

    // WHEN the card renders it
    const html = card(view);

    // THEN it is an update that stopped, with nothing downloaded and the
    //      current engine kept
    expect(textOf(html, "ai-engine-title")).toBe(
      fill(copy.refusal.title.update, { cause: copy.refusal.cause.list_altered }),
    );
    expect(html).toContain(copy.refusal.nothing_downloaded);
    expect(html).toContain(copy.refusal.current_kept);
  });

  test("later hides the offer until the channel offers another version", async () => {
    // GIVEN an update offered
    const { engine } = runtime({ engines: installed("b7012") });
    await engine.refresh();

    // WHEN it is put off
    engine.later();
    const hidden = cardView(get(engine));
    const next = cardView({ ...get(engine), engines: installed("b7100") });

    // THEN the card is gone for that version, and back for the next one
    expect(hidden).toBeNull();
    expect(next?.status).toBe("update_available");
  });
});

describe("Accelerated engine - no proposal", () => {
  const idle = (engines: EnginesResponse | null): AcceleratedEngineState => ({
    engines,
    installation: { state: "idle" },
    later: [],
    etaSeconds: null,
    loadedCount: 0,
    busy: false,
  });

  test("a computer without an NVIDIA card is offered nothing", async () => {
    // GIVEN the route says the engine is not proposable and lists no update
    const { engine } = runtime({
      engines: proposable({ cuda_proposable: false, nvidia_card: null, updates: [] }),
    });

    // WHEN the AI screen reads it
    await engine.refresh();

    // THEN there is no card, and the list is not fetched for nothing
    expect(cardView(get(engine))).toBeNull();
    expect(invoked("check_engine_updates")).toHaveLength(0);
  });

  test("an engine installed and up to date is offered nothing", () => {
    // GIVEN the installed build is the newest of the channel
    // WHEN the card is decided
    // THEN there is none, nor before the engines are read
    expect(cardView(idle(installed("b6870")))).toBeNull();
    expect(cardView(idle(null))).toBeNull();
  });

  test("a proposal put off stays hidden", async () => {
    // GIVEN a proposal
    const { engine } = runtime({ engines: proposable() });
    await engine.refresh();

    // WHEN it is put off
    engine.later();

    // THEN the card is gone
    expect(cardView(get(engine))).toBeNull();
  });
});

describe("Accelerated engine - the models already loaded", () => {
  test("an installed engine offers to reload the models in memory, and reloads them", async () => {
    // GIVEN an installation that just ended with two models in memory, one
    //       of them answering
    const { engine } = runtime({
      engines: installed("b6870"),
      initial: { state: "installed", kind: "install" },
      loaded: [
        loadedModel(),
        loadedModel({ modelPath: "/models/busy.gguf", modelId: "busy", activeRequests: 1 }),
      ],
    });
    await engine.refresh();
    const html = card(cardView(get(engine)));

    // THEN the card says the loaded models keep their engine and offers the reload
    expect(textOf(html, "ai-engine-body")).toBe(copy.installed.body_loaded);
    expect(textOf(html, "ai-engine-reload")).toBe(copy.action.reload);

    // WHEN the models are reloaded
    const outcome = await engine.reloadModels();

    // THEN the idle one is unloaded and loaded again with its window, the
    //      answering one is left alone, and the card stays for it
    expect(outcome).toEqual({ reloaded: 1, busy: 1 });
    expect(invoked("unload_loaded_model")).toEqual([{ modelPath: "/models/qwen3-8b-q4.gguf" }]);
    expect(invoked("load_model")).toEqual([
      { request: { modelId: "qwen3-8b-q4", nCtx: 8192, confirmed: true } },
    ]);
    expect(invoked("dismiss_engine_installation")).toHaveLength(0);
  });

  test("without a model in memory the installed card has no button", () => {
    // GIVEN an installed engine and nothing loaded
    const view = cardView({
      engines: installed("b6870"),
      installation: { state: "installed", kind: "install" },
      later: [],
      etaSeconds: null,
      loadedCount: 0,
      busy: false,
    });

    // WHEN the card renders it
    const html = card(view);

    // THEN it only says the next load uses the card
    expect(textOf(html, "ai-engine-body")).toBe(copy.installed.body);
    expect(html).not.toContain("<button");
  });
});

describe("Accelerated engine - quantities", () => {
  test("the time left follows the rate measured, and waits for a rate", () => {
    // GIVEN a download that received 80 of 412 MB in one minute
    const start = { atMs: 0, received: 0 };

    // WHEN what is left is computed, then before anything arrived
    const left = remainingSeconds(start, { atMs: 60_000, received: 80 * MB }, 412 * MB);
    const unknown = remainingSeconds(start, { atMs: 500, received: 80 * MB }, 412 * MB);
    const stalled = remainingSeconds(start, { atMs: 60_000, received: 0 }, 412 * MB);

    // THEN it is about four minutes, and unknown without a rate
    expect(left).toBe(249);
    expect(unknown).toBeNull();
    expect(stalled).toBeNull();
  });

  test("sizes are whole megabytes and days are written out", () => {
    // GIVEN sizes and days as the route gives them
    // WHEN they are written for a French reader
    // THEN a size is at least one megabyte, and only a real day is a day
    expect(megabytes(412_345_678, "fr")).toBe("412");
    expect(megabytes(10, "fr")).toBe("1");
    expect(releaseDay("2026-09-28", "fr")).toBe("28 septembre 2026");
    expect(releaseDay("2026-09-28", "en")).toBe("September 28, 2026");
    expect(releaseDay(null, "fr")).toBeNull();
    expect(releaseDay("b7012", "fr")).toBeNull();
  });
});
