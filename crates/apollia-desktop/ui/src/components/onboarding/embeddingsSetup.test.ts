import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, test, expect } from "vitest";
import { get } from "svelte/store";
import en from "$lib/i18n/en.json";
import fr from "$lib/i18n/fr.json";
import glossary from "$lib/i18n/glossary.json";
import type { DownloadProgress } from "$lib/ipc/models";
import type { EmbeddingModel, EmbeddingsRecommendation } from "$lib/ipc/embeddings";
import {
  UNAVAILABLE_WITHOUT_EMBEDDINGS,
  createEmbeddingsSetup,
  downloadRequest,
  embeddingsStepView,
  entryForFile,
  reasonKey,
  sizeInMb,
  type EmbeddingsSetupDeps,
} from "./embeddingsSetup";
import { ONBOARDING_STEPS } from "./onboardingSteps";

// Vitest runs in `node`: the DOM is exercised by the gestural corpus of
// scripts/automation. These tests drive the object the component draws, with
// the IPC replaced by a recorder, and read the template for what must be in it.

const SMALL: EmbeddingModel = {
  id: "multilingual-e5-small",
  label: "Multilingual E5 small",
  publisher: "intfloat",
  license: "mit",
  repo: "cstr/multilingual-e5-small-GGUF",
  file: "multilingual-e5-small-q8_0.gguf",
  quantization: "Q8_0",
  size_bytes: 131_624_960,
  sha256: "0a34067a40f25d3149b36885faa62bee0e5284d0f9edc102acfc00e115d953e8",
  dimensions: 384,
  context_length: 512,
};

const LARGE: EmbeddingModel = {
  ...SMALL,
  id: "qwen3-embedding-0.6b",
  label: "Qwen3 Embedding 0.6B",
  repo: "Qwen/Qwen3-Embedding-0.6B-GGUF",
  file: "Qwen3-Embedding-0.6B-Q8_0.gguf",
  size_bytes: 639_150_592,
  dimensions: 1024,
};

const RECOMMENDATION: EmbeddingsRecommendation = {
  models: [SMALL, LARGE],
  recommended: SMALL.id,
  reason: {
    kind: "no_room_beside",
    language_gb: 6.1,
    larger: LARGE.id,
    larger_gb: 1.2,
    capacity_gb: 7,
  },
};

/** A simulated IPC that records every call in order. */
function simulatedIpc(overrides: Partial<EmbeddingsSetupDeps> = {}) {
  const calls: string[] = [];
  const deps: EmbeddingsSetupDeps = {
    recommend: async (bytes) => {
      calls.push(`recommend:${bytes}`);
      return RECOMMENDATION;
    },
    current: async () => ({ model: null, dimensions: null }),
    startDownload: async (request) => {
      calls.push(`download:${request.repo_id}:${JSON.stringify(request.selector)}`);
      return "dl-1";
    },
    cancelDownload: async (id) => {
      calls.push(`cancel:${id}`);
    },
    importFile: async (path, extensions) => {
      calls.push(`import:${path}:${extensions.join(",")}`);
      return `/home/u/.apollia/models/${path.split("/").pop()}`;
    },
    setDefault: async (model) => {
      calls.push(`default:${model}`);
      return { model, installed: true, path: `/home/u/.apollia/models/${model}.gguf` };
    },
    onProgress: async () => {
      calls.push("listen");
      return () => {};
    },
    ...overrides,
  };
  return { deps, calls };
}

function progress(status: DownloadProgress["status"], id = "dl-1"): DownloadProgress {
  return {
    id,
    downloaded_bytes: 60_000_000,
    total_bytes: SMALL.size_bytes,
    speed_bps: 4_000_000,
    dest_path: `/home/u/.apollia/models/${SMALL.file}`,
    status,
  };
}

function templateSource(file: string): string {
  return readFileSync(fileURLToPath(new URL(file, import.meta.url)), "utf8");
}

describe("embeddings step - the offer", () => {
  test("offers the model the recommendation route names, sized beside the language model", async () => {
    // GIVEN a machine with no embeddings model, and a 4.9 GB language model
    // wired at the previous step
    const { deps, calls } = simulatedIpc();
    const setup = createEmbeddingsSetup(deps);

    // WHEN the step opens
    await setup.load(4_920_000_000);

    // THEN the route was asked with that size, and its recommended entry is
    // the one offered, with its reason
    expect(calls).toEqual(["recommend:4920000000"]);
    const state = get(setup);
    expect(state.phase).toBe("offer");
    expect(state.model?.id).toBe(SMALL.id);
    expect(reasonKey(state.reason)).toBe("onboarding.embeddings.reason_light");
    // AND the card shows the download and the import from a file
    const view = embeddingsStepView(state, true, false);
    expect(view.showModelCard).toBe(true);
    expect(view.showDownload).toBe(true);
    expect(view.downloadDisabled).toBe(false);
    expect(view.showImport).toBe(true);
    expect(sizeInMb(SMALL.size_bytes)).toBe(132);
  });

  test("a model already installed opens the step as ready", async () => {
    // GIVEN a machine whose default embeddings model is installed
    const { deps } = simulatedIpc({
      current: async () => ({ model: LARGE.id, dimensions: 1024 }),
    });
    const setup = createEmbeddingsSetup(deps);

    // WHEN the step opens
    await setup.load(null);

    // THEN it names that model as installed and lets the person continue
    const state = get(setup);
    expect(state.phase).toBe("ready");
    expect(state.model?.id).toBe(LARGE.id);
    const view = embeddingsStepView(state, true, false);
    expect(view.showReady).toBe(true);
    expect(view.showDownload).toBe(false);
    expect(view.continueDisabled).toBe(false);
    expect(view.showSkip).toBe(false);
  });

  test("a registry that cannot be read leaves the step passable", async () => {
    // GIVEN a recommendation route that fails
    const { deps } = simulatedIpc({
      recommend: async () => {
        throw new Error("the shipped embeddings table is invalid");
      },
    });
    const setup = createEmbeddingsSetup(deps);

    // WHEN the step opens
    await setup.load(null);

    // THEN nothing is offered, the failure is kept, and passing stays possible
    const state = get(setup);
    expect(state.phase).toBe("unavailable");
    expect(state.failure).toEqual({
      kind: "recommendation",
      detail: "the shipped embeddings table is invalid",
    });
    const view = embeddingsStepView(state, true, false);
    expect(view.showDownload).toBe(false);
    expect(view.showSkip).toBe(true);
  });
});

describe("embeddings step - the download", () => {
  test("goes through the verified variant download of the pinned file", () => {
    // GIVEN the offered entry of the registry
    // WHEN its download request is built
    const request = downloadRequest(SMALL);

    // THEN it names the repository and the one GGUF file the registry pins,
    // which the variant download checks against the published SHA-256
    expect(request).toEqual({
      repo_id: "cstr/multilingual-e5-small-GGUF",
      selector: { kind: "gguf", file: "multilingual-e5-small-q8_0.gguf" },
      hf_token: null,
      dest_dir: null,
    });
  });

  test("the downloaded model becomes the default embeddings model", async () => {
    // GIVEN the step open on its offer
    const { deps, calls } = simulatedIpc();
    const setup = createEmbeddingsSetup(deps);
    await setup.load(null);
    calls.length = 0;

    // WHEN the person downloads, and the download reports progress then ends
    await setup.download();
    await setup.handleProgress(progress("in_progress"));
    const during = get(setup);
    await setup.handleProgress(progress("completed"));

    // THEN the listener was set before the download started, and the model
    // was made the default only once its file had landed
    expect(calls).toEqual([
      "listen",
      'download:cstr/multilingual-e5-small-GGUF:{"kind":"gguf","file":"multilingual-e5-small-q8_0.gguf"}',
      "default:multilingual-e5-small",
    ]);
    // AND the step moved from the progress to the installed model
    expect(during.phase).toBe("downloading");
    expect(during.progress?.downloaded_bytes).toBe(60_000_000);
    expect(embeddingsStepView(during, true, false).continueDisabled).toBe(false);
    const after = get(setup);
    expect(after.phase).toBe("ready");
    expect(after.failure).toBeNull();
  });

  test("the progress of another download is ignored", async () => {
    // GIVEN a download of the embeddings model in flight
    const { deps, calls } = simulatedIpc();
    const setup = createEmbeddingsSetup(deps);
    await setup.load(null);
    await setup.download();
    calls.length = 0;

    // WHEN the language model, downloading beside it, completes
    await setup.handleProgress(progress("completed", "dl-language"));

    // THEN nothing is made the default and the step still downloads
    expect(calls).toEqual([]);
    expect(get(setup).phase).toBe("downloading");
  });

  test("a failed download puts the offer back with its error", async () => {
    // GIVEN a download in flight
    const { deps, calls } = simulatedIpc();
    const setup = createEmbeddingsSetup(deps);
    await setup.load(null);
    await setup.download();

    // WHEN it fails
    await setup.handleProgress(progress("failed"));

    // THEN no default was set, and the model is offered again
    expect(calls).not.toContain("default:multilingual-e5-small");
    const state = get(setup);
    expect(state.phase).toBe("offer");
    expect(state.failure?.kind).toBe("download");
  });

  test("cancelling a download that no longer runs puts the offer back", async () => {
    // GIVEN a download whose end never reached the window, so that the
    // application refuses to cancel an identifier it no longer knows
    const { deps } = simulatedIpc({
      cancelDownload: async () => {
        throw new Error("cancel failed: download not found");
      },
    });
    const setup = createEmbeddingsSetup(deps);
    await setup.load(null);
    await setup.download();

    // WHEN the person cancels
    await setup.cancel();

    // THEN the step does not stay on a progress bar nothing will move
    expect(get(setup).phase).toBe("offer");
    expect(embeddingsStepView(get(setup), true, false).showSkip).toBe(true);
  });

  test("a download the Hub refuses to start is reported in its own words", async () => {
    // GIVEN a Hub that cannot be reached when the download is prepared
    const { deps } = simulatedIpc({
      startDownload: async () => {
        throw new Error("cannot prepare the download, refusing it: network");
      },
    });
    const setup = createEmbeddingsSetup(deps);
    await setup.load(null);

    // WHEN the person downloads
    await setup.download();

    // THEN the offer is back, with the refusal
    const state = get(setup);
    expect(state.phase).toBe("offer");
    expect(state.failure).toEqual({
      kind: "download",
      detail: "cannot prepare the download, refusing it: network",
    });
  });
});

describe("embeddings step - passing", () => {
  test("passing shows the warning, and the onboarding continues from it", async () => {
    // GIVEN the step open on its offer
    const { deps, calls } = simulatedIpc();
    const setup = createEmbeddingsSetup(deps);
    await setup.load(null);
    calls.length = 0;

    // WHEN the person asks to pass
    const before = embeddingsStepView(get(setup), true, false);
    const asked = embeddingsStepView(get(setup), true, true);

    // THEN "Skip" was offered, and asking replaces it by the warning and one
    // button that continues without the model
    expect(before.showSkip).toBe(true);
    expect(before.showSkipWarning).toBe(false);
    expect(before.continueDisabled).toBe(true);
    expect(asked.showSkipWarning).toBe(true);
    expect(asked.showContinueWithout).toBe(true);
    expect(asked.showSkip).toBe(false);
    expect(asked.showContinue).toBe(false);
    // AND passing sets no default: search goes on with keywords alone
    expect(calls).toEqual([]);
  });

  test("the warning lists both unavailable functions, keywords, and the AI screen", () => {
    // GIVEN the keys the warning renders, in both catalogues
    for (const catalogue of [fr, en]) {
      const texts = catalogue.onboarding.embeddings;
      const words = catalogue === fr ? glossary.fr : glossary.en;
      const meaning = words.search_by_meaning.toLowerCase();
      const sources = words.objects.source.toLowerCase();

      // WHEN its sentences are read
      const lost = UNAVAILABLE_WITHOUT_EMBEDDINGS.map(
        (key) => texts[key.split(".").pop() as keyof typeof texts],
      );

      // THEN the two functions are search by meaning in the sources and in
      // the memory of the agents
      expect(lost).toHaveLength(2);
      expect(lost[0]).toContain(meaning);
      expect(lost[0]).toContain(sources);
      expect(lost[1]).toContain(meaning);
      expect(lost[1]).not.toContain(sources);
      expect(UNAVAILABLE_WITHOUT_EMBEDDINGS[1]).toBe("onboarding.embeddings.skip_lost_memory");
      // AND it says what remains, and names the screen where to install the
      // model later
      expect(texts.skip_keywords.trim()).not.toBe("");
      expect(texts.skip_later).toContain(words.screens.ai);
    }
    // AND each sentence is written in its own language
    expect(fr.onboarding.embeddings.skip_keywords).not.toBe(en.onboarding.embeddings.skip_keywords);
    expect(fr.onboarding.embeddings.skip_lost_memory).not.toBe(
      en.onboarding.embeddings.skip_lost_memory,
    );
  });

  test("the template draws the warning, its list and the button that continues", () => {
    // GIVEN the source of the step
    const source = templateSource("./OnboardingEmbeddingsStep.svelte");

    // WHEN its regions are read
    // THEN the warning, each lost function and both ways out are anchored
    for (const testid of [
      "embeddings-skip",
      "embeddings-skip-warning",
      "embeddings-skip-lost",
      "embeddings-continue-without",
      "embeddings-continue",
    ]) {
      expect(source).toContain(`data-testid="${testid}"`);
    }
    expect(source).toContain("UNAVAILABLE_WITHOUT_EMBEDDINGS");
    // AND both buttons that leave the step call the same continuation
    expect(source.match(/onclick=\{onnext\}/g)).toHaveLength(2);
  });
});

describe("embeddings step - without the network", () => {
  test("the download is disabled with its reason, the import stays possible", async () => {
    // GIVEN the step open on its offer, and the network cut
    const { deps, calls } = simulatedIpc();
    const setup = createEmbeddingsSetup(deps);
    await setup.load(null);
    calls.length = 0;

    // WHEN the step is drawn offline
    const view = embeddingsStepView(get(setup), false, false);

    // THEN the download is shown disabled, with the sentence that says why
    expect(view.showDownload).toBe(true);
    expect(view.downloadDisabled).toBe(true);
    expect(view.downloadDisabledReasonKey).toBe("onboarding.embeddings.offline");
    expect(en.onboarding.embeddings.offline).toContain("Offline");
    expect(fr.onboarding.embeddings.offline.trim()).not.toBe("");
    expect(fr.onboarding.embeddings.offline).not.toBe(en.onboarding.embeddings.offline);
    // AND the import is still offered and enabled
    expect(view.showImport).toBe(true);
    expect(view.importDisabled).toBe(false);
    // AND the template ties that sentence to the button it explains
    const source = templateSource("./OnboardingEmbeddingsStep.svelte");
    expect(source).toContain("disabled={view.downloadDisabled}");
    expect(source).toContain('id="embeddings-download-reason"');
    expect(source).toContain("{$t(view.downloadDisabledReasonKey)}");
    // AND the recommendation itself needed no network: it came from the route
    expect(get(setup).model?.id).toBe(SMALL.id);
  });

  test("a file of the registry imported from a disk becomes the default", async () => {
    // GIVEN the step open offline, and the file of the offered model on a key
    const { deps, calls } = simulatedIpc();
    const setup = createEmbeddingsSetup(deps);
    await setup.load(null);
    calls.length = 0;

    // WHEN the person imports it
    await setup.importFrom("/Volumes/KEY/multilingual-e5-small-q8_0.gguf");

    // THEN it was imported as a GGUF, then made the default, with no download
    expect(calls).toEqual([
      "import:/Volumes/KEY/multilingual-e5-small-q8_0.gguf:gguf",
      "default:multilingual-e5-small",
    ]);
    const state = get(setup);
    expect(state.phase).toBe("ready");
    expect(state.importing).toBe(false);
  });

  test("the other entry of the registry is accepted too, and named", async () => {
    // GIVEN the small model offered, and the file of the larger one on a key
    const { deps, calls } = simulatedIpc();
    const setup = createEmbeddingsSetup(deps);
    await setup.load(null);
    calls.length = 0;

    // WHEN the person imports it
    await setup.importFrom("C:\\Users\\u\\Downloads\\Qwen3-Embedding-0.6B-Q8_0.gguf");

    // THEN that entry, not the offered one, is the default and the model shown
    expect(calls.at(-1)).toBe("default:qwen3-embedding-0.6b");
    expect(get(setup).model?.id).toBe(LARGE.id);
  });

  test("a file that is no embeddings model of the registry is refused before any copy", async () => {
    // GIVEN the step open on its offer
    const { deps, calls } = simulatedIpc();
    const setup = createEmbeddingsSetup(deps);
    await setup.load(null);
    calls.length = 0;

    // WHEN the person picks a language model
    await setup.importFrom("/Users/u/Downloads/Phi-3-mini-Q4.gguf");

    // THEN nothing was copied, no default was set, and the step says which
    // file it refused
    expect(calls).toEqual([]);
    const state = get(setup);
    expect(state.phase).toBe("offer");
    expect(state.failure).toEqual({ kind: "not_offered", detail: "Phi-3-mini-Q4.gguf" });
    expect(entryForFile([SMALL, LARGE], "/x/Phi-3-mini-Q4.gguf")).toBeNull();
  });
});

describe("embeddings step - its place and its words", () => {
  test("it is the third step, right after the language model", () => {
    // GIVEN the steps of the first-launch modal
    const ids = ONBOARDING_STEPS.map((step) => step.id);

    // WHEN the step after the AI setup is read
    // THEN it is the embeddings step, third of the list
    expect(ids.indexOf("embeddings")).toBe(2);
    expect(ids[ids.indexOf("ai-setup") + 1]).toBe("embeddings");
    // AND the modal mounts it between the AI setup and the conversation
    const modal = templateSource("./OnboardingModal.svelte");
    expect(modal).toContain('{:else if currentStep === "embeddings"}');
    expect(modal).toContain("<OnboardingEmbeddingsStep");
  });

  test("its label and explanation are not those of the language model", () => {
    for (const catalogue of [fr, en]) {
      // GIVEN the texts of the two steps in one catalogue
      const embeddings = catalogue.onboarding.embeddings;
      const language = catalogue.onboarding.ai_setup;

      // WHEN their titles and explanations are compared
      // THEN they differ, and the step has its own name in the stepper
      expect(embeddings.title).not.toBe(language.title);
      expect(embeddings.subtitle).not.toBe(language.subtitle);
      expect(catalogue.onboarding_modal.step_search).not.toBe(
        catalogue.onboarding_modal.step_ai,
      );
      // AND the card says the size and that the model runs on this machine
      expect(embeddings.size_runs_locally).toContain("{mb}");
      expect(embeddings.size_runs_locally).toContain(" · ");
      // AND the two means of getting it each have their own label
      expect(embeddings.download.trim()).not.toBe("");
      expect(embeddings.import.trim()).not.toBe("");
      expect(embeddings.import).not.toBe(embeddings.download);
    }
    expect(en.onboarding.embeddings.size_runs_locally).toContain("runs on your computer");
    expect(en.onboarding.embeddings.download).toBe("Download");
    expect(en.onboarding.embeddings.import).toBe("Import from a file");
  });

  test("the words for search by meaning are those of the glossary", () => {
    // GIVEN the glossary, which the other screens read the same words from
    const pairs = [
      [glossary.fr, fr],
      [glossary.en, en],
    ] as const;

    for (const [words, catalogue] of pairs) {
      // WHEN the passing warning and the import title are read
      const texts = catalogue.onboarding.embeddings;

      // THEN they use the glossary's wording, in its sentence case
      expect(texts.skip_lost_sources.toLowerCase()).toContain(
        words.search_by_meaning.toLowerCase(),
      );
      expect(texts.import_title.toLowerCase()).toContain(
        words.search_by_meaning_model.toLowerCase(),
      );
    }
  });

  test("the larger entry is explained by the room it has", () => {
    // GIVEN a recommendation made because the entry fits beside the language model
    // WHEN its reason is turned into a sentence
    const key = reasonKey({
      kind: "fits_beside",
      language_gb: 4,
      embeddings_gb: 1.2,
      capacity_gb: 24,
    });

    // THEN it is the sentence of the roomy case, and no reason gives none
    expect(key).toBe("onboarding.embeddings.reason_roomy");
    expect(reasonKey(null)).toBeNull();
  });
});
