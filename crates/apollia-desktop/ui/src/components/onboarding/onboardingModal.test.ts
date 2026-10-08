import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, test, expect } from "vitest";
import en from "$lib/i18n/en.json";
import fr from "$lib/i18n/fr.json";
import { downloadNotice, type LlmDownloadState } from "./llmDownload";

// Vitest runs in `node`: the modal is not mounted here, its gestures are
// played by the automation recipes. These tests read the templates for what
// must be in them: which steps can be passed, where "configure later" lives,
// and what the first step no longer says.

function source(file: string): string {
  return readFileSync(fileURLToPath(new URL(file, import.meta.url)), "utf8");
}

/** The markup between the opening and the closing tag that carry `marker`. */
function region(text: string, open: string, close: string): string {
  const start = text.indexOf(open);
  const end = text.indexOf(close, start);
  expect(start, open).toBeGreaterThan(-1);
  expect(end, close).toBeGreaterThan(start);
  return text.slice(start, end);
}

describe("first launch - configure later holds on every step", () => {
  const modal = source("./OnboardingModal.svelte");
  const header = region(modal, '<header class="onboarding-card-header">', "</header>");

  test("the header carries it, outside any step condition", () => {
    // GIVEN the header of the modal, drawn on every step
    // WHEN its controls are read
    // THEN "configure later" is there, wired to the skip that is recorded
    expect(header).toContain('data-testid="onboarding-skip"');
    expect(header).toContain("onclick={handleSkip}");
    expect(header).toContain('$t("onboarding_modal.skip_later")');
    // AND no step decides whether it is shown, the AI step included
    expect(header).not.toContain("currentStep");
  });

  test("the header keeps the download of the model in sight", () => {
    // GIVEN the same header
    // WHEN the notice of the download is looked for
    // THEN it is drawn from the object that outlives the AI step
    expect(header).toContain('data-testid="onboarding-model-progress"');
    expect(modal).toContain("downloadNotice($llmDownload)");
  });

  test("every sentence the header may show exists in both languages", () => {
    // GIVEN every state a download can be in
    const states: LlmDownloadState[] = (
      ["downloading", "installing", "ready", "failed"] as const
    ).map((phase) => ({
      phase,
      filename: "m.gguf",
      sizeBytes: null,
      progress: null,
      wiredPath: null,
      failure: null,
    }));
    const keys = states.map((state) => downloadNotice(state)?.key ?? "");
    keys.push("onboarding_modal.model_percent", "onboarding_modal.step_count");

    // WHEN each key is looked up in the two catalogues
    // THEN each is there and says something
    for (const catalogue of [fr, en]) {
      const texts = catalogue.onboarding_modal as Record<string, string>;
      for (const key of keys) {
        expect(texts[key.replace("onboarding_modal.", "")]?.trim(), key).toBeTruthy();
      }
    }
  });
});

describe("first launch - every step but the AI one can be passed", () => {
  test("the embeddings step and the conversation each carry their skip", () => {
    // GIVEN the templates of the steps after the AI setup
    const embeddings = source("./OnboardingEmbeddingsStep.svelte");
    const modal = source("./OnboardingModal.svelte");
    const review = source("./OnboardingProfileReview.svelte");

    // WHEN their controls are read
    // THEN the embeddings step can be passed
    expect(embeddings).toContain('data-testid="embeddings-skip"');
    // AND the questions of the conversation can be passed, where there is one:
    // a profile completed later from the settings
    const footer = region(
      modal,
      '{#if currentStep === "chat" && enriching}\n      <!--',
      "{/if}\n  </div>",
    );
    expect(footer).toContain('data-testid="onboarding-skip-optional"');
    // AND the profile form, which the first launch opens on, can be left unsaved
    expect(modal).toContain("<OnboardingModelTest");
    expect(source("./OnboardingModelTest.svelte")).toContain("<OnboardingProfileReview proposal={null}");
    expect(review).toContain('data-testid="onboarding-review-skip"');
  });

  test("the language step is never closed: there is nothing to pass", () => {
    // GIVEN the template of the first step
    const welcome = source("./OnboardingWelcome.svelte");
    const cta = region(welcome, "<Button", "</Button>");

    // WHEN its continue button is read
    // THEN it is the button of the step and nothing disables it
    expect(cta).toContain('data-testid="onboarding-welcome-cta"');
    expect(cta).not.toContain("disabled");
  });

  test("the AI step is handed no way of being passed", () => {
    // GIVEN the place where the modal mounts the AI step
    const modal = source("./OnboardingModal.svelte");
    const mount = region(modal, "<OnboardingAiSetup", "/>");

    // WHEN the callbacks it is given are read
    // THEN it can go on or go back, and nothing else
    expect(mount).toContain("onnext=");
    expect(mount).toContain("onback=");
    expect(mount).not.toContain("onskip");
  });
});

describe("first launch - the language step", () => {
  test("the switch between French and English stays", () => {
    // GIVEN the template of the first step
    const welcome = source("./OnboardingWelcome.svelte");

    // WHEN its language controls are read
    // THEN the toggle offers every supported language and applies the choice
    expect(welcome).toContain('data-testid="onboarding-lang-toggle"');
    expect(welcome).toContain("{#each SUPPORTED_LOCALES as code}");
    expect(welcome).toContain("data-testid={`onboarding-lang-${code}`}");
    expect(welcome).toContain("setLocale(next);");
  });

  test("its texts name no engine, no provider, no technique and no duration", () => {
    // GIVEN the texts of the first step in the two catalogues
    const banned = [/llama\.cpp/i, /anthropic/i, /openai/i, /react/i, /3 minutes/i, /runtime/i];

    for (const catalogue of [fr, en]) {
      const texts = Object.values(catalogue.onboarding_welcome as Record<string, string>);

      // WHEN each is read
      // THEN there is something to read, and none of the words is in it
      expect(texts.length).toBeGreaterThan(0);
      for (const text of texts) {
        for (const word of banned) {
          expect(text, String(word)).not.toMatch(word);
        }
      }
    }
  });

  test("every text the step shows exists in both languages", () => {
    // GIVEN the keys the template of the first step reads
    const welcome = source("./OnboardingWelcome.svelte");
    const keys = [...welcome.matchAll(/"onboarding_welcome\.([a-z_]+)"/g)].map((m) => m[1]);

    // WHEN each is looked up
    // THEN each is in the two catalogues, and the catalogues hold no other
    expect(keys.length).toBeGreaterThan(0);
    for (const catalogue of [fr, en]) {
      expect(Object.keys(catalogue.onboarding_welcome).sort()).toEqual([...keys].sort());
    }
  });
});
