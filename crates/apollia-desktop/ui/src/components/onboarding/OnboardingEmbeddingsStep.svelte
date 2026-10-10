<!--
  Onboarding step 3, the two models beside the one that converses: the one
  search by meaning uses, and the one dictation uses.

  For each: the model recommended for this machine, its size, a download and
  an import from a file, then a trial on the spot, so the person sees the
  model answer before leaving the step. Passing the step shows what is lost
  without the search model and where to install it later; dictation is never
  required. The decisions are in `embeddingsSetup.ts` and `dictationSetup.ts`;
  this component and `OnboardingDictationSetup` draw them.
-->
<script lang="ts">
  import { onMount } from "svelte";
  import { get } from "svelte/store";
  import { t } from "svelte-i18n";
  import { open as openFilePicker } from "@tauri-apps/plugin-dialog";
  import { AlertCircle, Check, Download, FileSearch, Upload, WifiOff, X } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { ProgressBar, Spinner } from "$lib/components/ui/progress";
  import { ErrorBanner } from "$lib/components/operator";
  import { embeddingsSetup } from "./embeddingsSetupLive";
  import OnboardingDictationSetup from "./OnboardingDictationSetup.svelte";
  import OnboardingEmbeddingsTest from "./OnboardingEmbeddingsTest.svelte";
  import {
    UNAVAILABLE_WITHOUT_EMBEDDINGS,
    embeddingsStepView,
    failureKey,
    reasonKey,
    sizeInMb,
  } from "./embeddingsSetup";
  import { dlBytes, dlPct, dlSpeed, pickModelsDir } from "./onboardingFormat";
  import "./onboarding-ai-setup.css";

  interface Props {
    onnext: () => void;
    onback: () => void;
    /** Size of the language model wired at the previous step, when known. */
    languageModelBytes: number | null;
  }

  const { onnext, onback, languageModelBytes }: Props = $props();

  let online = $state(typeof navigator === "undefined" ? true : navigator.onLine);
  let skipAsked = $state(false);

  const view = $derived(embeddingsStepView($embeddingsSetup, online, skipAsked));
  const model = $derived($embeddingsSetup.model);
  const failure = $derived($embeddingsSetup.failure);
  const progress = $derived($embeddingsSetup.progress);
  const reason = $derived(reasonKey($embeddingsSetup.reason));

  onMount(() => {
    void embeddingsSetup.load(languageModelBytes);
    const sync = (): void => {
      online = navigator.onLine;
    };
    window.addEventListener("online", sync);
    window.addEventListener("offline", sync);
    return () => {
      window.removeEventListener("online", sync);
      window.removeEventListener("offline", sync);
    };
  });

  async function importFromFile(): Promise<void> {
    const selected = await openFilePicker({
      multiple: false,
      filters: [{ name: "GGUF", extensions: ["gguf"] }],
      title: get(t)("onboarding.embeddings.import_title"),
      defaultPath: await pickModelsDir(),
    });
    if (!selected) return;
    const path = typeof selected === "string" ? selected : (selected as { path: string }).path;
    await embeddingsSetup.importFrom(path);
  }
</script>

<div class="ai-setup" data-testid="onboarding-embeddings">
  <header class="setup-header">
    <h2 class="setup-title">{$t("onboarding.embeddings.title")}</h2>
    <p class="setup-subtitle">{$t("onboarding.embeddings.subtitle")}</p>
  </header>

  {#if $embeddingsSetup.phase === "loading"}
    <div class="scan-loading">
      <Spinner size={18} />
      <span>{$t("onboarding.embeddings.loading")}</span>
    </div>
  {:else if $embeddingsSetup.phase === "unavailable"}
    <p class="empty-hint" data-testid="embeddings-unavailable">
      {$t("onboarding.embeddings.unavailable")}
    </p>
  {/if}

  {#if view.showModelCard && model}
    <section class="setup-section" data-testid="embeddings-model-card">
      <div class="section-header">
        <FileSearch size={14} strokeWidth={2} class="text-primary" />
        <span class="section-title" data-testid="embeddings-model-name">{model.label}</span>
        {#if view.showReady}
          <span class="section-badge-ok" data-testid="embeddings-ready">
            <Check size={10} strokeWidth={2.5} /> {$t("onboarding.embeddings.ready")}
          </span>
        {:else}
          <span class="badge-recommended" data-testid="embeddings-recommended">
            {$t("onboarding.embeddings.recommended")}
          </span>
        {/if}
      </div>
      <p class="text-body-xs text-muted-foreground" data-testid="embeddings-model-size">
        {$t("onboarding.embeddings.size_runs_locally", {
          values: { mb: sizeInMb(model.size_bytes) },
        })}
      </p>
      {#if reason && !view.showReady}
        <p class="text-caption leading-snug text-muted-foreground" data-testid="embeddings-reason">
          {$t(reason)}
        </p>
      {/if}

      {#if view.showProgress}
        <div class="download-block" data-testid="embeddings-download-progress">
          <div class="dl-header">
            <span class="dl-filename">{model.file}</span>
            <Button
              variant="ghost"
              size="sm"
              class="btn-cancel-dl"
              onclick={() => void embeddingsSetup.cancel()}
              aria-label={$t("onboarding.embeddings.cancel_download")}
              data-testid="embeddings-download-cancel"
            >
              <X size={12} strokeWidth={2} />
            </Button>
          </div>
          <ProgressBar value={progress ? dlPct(progress) : undefined} size="sm" variant="primary" />
          {#if progress}
            <div class="dl-meta">
              <span>{dlBytes(progress)}</span>
              <span>{dlSpeed(progress.speed_bps)}</span>
            </div>
          {/if}
        </div>
      {/if}

      {#if view.showDownload || view.showImport}
        <div class="load-model-row">
          {#if view.showDownload}
            <Button
              variant="default"
              size="sm"
              onclick={() => void embeddingsSetup.download()}
              disabled={view.downloadDisabled}
              aria-describedby={view.downloadDisabledReasonKey ? "embeddings-download-reason" : undefined}
              data-testid="embeddings-download"
            >
              <Download size={12} strokeWidth={2} />
              {$t("onboarding.embeddings.download")}
            </Button>
          {/if}
          {#if view.showImport}
            <Button
              variant="ghost"
              size="sm"
              onclick={() => void importFromFile()}
              disabled={view.importDisabled}
              loading={$embeddingsSetup.importing}
              data-testid="embeddings-import"
            >
              <Upload size={12} strokeWidth={2} />
              {$t("onboarding.embeddings.import")}
            </Button>
          {/if}
        </div>
      {/if}

      {#if view.downloadDisabledReasonKey}
        <p class="empty-hint" id="embeddings-download-reason">
          <WifiOff size={12} strokeWidth={2} class="inline-block" />
          {$t(view.downloadDisabledReasonKey)}
        </p>
      {/if}

      {#if view.showReady}
        <OnboardingEmbeddingsTest />
      {/if}
    </section>
  {/if}

  {#if failure}
    <p class="inline-error" role="alert" data-testid="embeddings-error">
      <AlertCircle size={12} />
      {#if failure.kind === "not_offered"}
        {$t(failureKey(failure.kind), { values: { file: failure.detail ?? "" } })}
      {:else}
        {$t(failureKey(failure.kind))}{failure.detail ? ` ${failure.detail}` : ""}
      {/if}
    </p>
  {/if}

  {#if view.showSkipWarning}
    <ErrorBanner tone="warning" data-testid="embeddings-skip-warning">
      <div>
        <p class="font-medium">{$t("onboarding.embeddings.skip_title")}</p>
        <ul class="mt-1 list-disc pl-4">
          {#each UNAVAILABLE_WITHOUT_EMBEDDINGS as key (key)}
            <li data-testid="embeddings-skip-lost">{$t(key)}</li>
          {/each}
        </ul>
        <p class="mt-1">{$t("onboarding.embeddings.skip_keywords")}</p>
        <p class="mt-1">{$t("onboarding.embeddings.skip_later")}</p>
      </div>
    </ErrorBanner>
  {/if}

  <OnboardingDictationSetup {online} />

  <footer class="setup-footer">
    <Button variant="ghost" size="sm" class="btn-secondary" onclick={onback} data-testid="embeddings-back">
      ← {$t("common.back")}
    </Button>
    <div class="footer-right">
      {#if view.showSkip}
        <Button
          variant="ghost"
          size="sm"
          class="btn-tertiary"
          onclick={() => (skipAsked = true)}
          data-testid="embeddings-skip"
        >
          {$t("onboarding.embeddings.skip")}
        </Button>
      {/if}
      {#if view.showContinueWithout}
        <Button variant="default" size="default" onclick={onnext} data-testid="embeddings-continue-without">
          {$t("onboarding.embeddings.continue_without")}
        </Button>
      {/if}
      {#if view.showContinue}
        <Button
          variant="primary-gradient"
          size="default"
          onclick={onnext}
          disabled={view.continueDisabled}
          data-testid="embeddings-continue"
        >
          {$t("common.continue")}
        </Button>
      {/if}
    </div>
  </footer>
</div>
