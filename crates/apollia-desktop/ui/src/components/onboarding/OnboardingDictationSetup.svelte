<!--
  Dictation in the first launch: the dictation model recommended for this
  machine, a download or an import from a file, then, once it is installed and
  dictation is on, the microphone and the same trial as Settings > Dictation.

  Never required: the person who does not dictate leaves the block as it is.
  The decisions are in `embeddingsSetup.ts` (the setup of a model) and
  `dictationSetup.ts` (what is particular to dictation).
-->
<script lang="ts">
  import { onMount } from "svelte";
  import { get } from "svelte/store";
  import { t } from "svelte-i18n";
  import { open as openFilePicker } from "@tauri-apps/plugin-dialog";
  import { AlertCircle, Check, Download, Mic, Upload, WifiOff, X } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { ProgressBar, Spinner } from "$lib/components/ui/progress";
  import { dictationSetup } from "./dictationSetupLive";
  import { DICTATION_EXTENSIONS } from "./dictationSetup";
  import { embeddingsStepView, sizeInMb } from "./embeddingsSetup";
  import { dlBytes, dlPct, dlSpeed, pickModelsDir } from "./onboardingFormat";
  import OnboardingDictation from "./OnboardingDictation.svelte";
  import "./onboarding-ai-setup.css";

  interface Props {
    online: boolean;
  }

  const { online }: Props = $props();

  const view = $derived(embeddingsStepView($dictationSetup, online, false));
  const model = $derived($dictationSetup.model);
  const failure = $derived($dictationSetup.failure);
  const progress = $derived($dictationSetup.progress);

  onMount(() => {
    void dictationSetup.load(null);
  });

  async function importFromFile(): Promise<void> {
    const selected = await openFilePicker({
      multiple: false,
      filters: [{ name: "Whisper", extensions: DICTATION_EXTENSIONS }],
      title: get(t)("onboarding.dictation_model.import_title"),
      defaultPath: await pickModelsDir(),
    });
    if (!selected) return;
    const path = typeof selected === "string" ? selected : (selected as { path: string }).path;
    await dictationSetup.importFrom(path);
  }
</script>

<div data-testid="onboarding-dictation-setup" data-phase={$dictationSetup.phase}>
  <header class="setup-header">
    <h2 class="setup-title">{$t("onboarding.dictation_model.title")}</h2>
    <p class="setup-subtitle">{$t("onboarding.dictation_model.subtitle")}</p>
  </header>

  {#if $dictationSetup.phase === "loading"}
    <div class="scan-loading">
      <Spinner size={18} />
      <span>{$t("onboarding.dictation_model.loading")}</span>
    </div>
  {:else if $dictationSetup.phase === "unavailable"}
    <p class="empty-hint" data-testid="dictation-unavailable">
      {$t("onboarding.dictation_model.unavailable")}
    </p>
  {/if}

  {#if view.showModelCard && model}
    <section class="setup-section" data-testid="dictation-model-card">
      <div class="section-header">
        <Mic size={14} strokeWidth={2} class="text-primary" />
        <span class="section-title" data-testid="dictation-model-name">{model.label}</span>
        {#if view.showReady}
          <span class="section-badge-ok" data-testid="dictation-ready">
            <Check size={10} strokeWidth={2.5} /> {$t("onboarding.embeddings.ready")}
          </span>
        {:else}
          <span class="badge-recommended" data-testid="dictation-recommended">
            {$t("onboarding.embeddings.recommended")}
          </span>
        {/if}
      </div>
      <p class="text-body-xs text-muted-foreground" data-testid="dictation-model-size">
        {$t("onboarding.embeddings.size_runs_locally", { values: { mb: sizeInMb(model.size_bytes) } })}
      </p>

      {#if view.showProgress}
        <div class="download-block" data-testid="dictation-download-progress">
          <div class="dl-header">
            <span class="dl-filename">{model.file}</span>
            <Button
              variant="ghost"
              size="sm"
              class="btn-cancel-dl"
              onclick={() => void dictationSetup.cancel()}
              aria-label={$t("onboarding.embeddings.cancel_download")}
              data-testid="dictation-download-cancel"
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
              onclick={() => void dictationSetup.download()}
              disabled={view.downloadDisabled}
              data-testid="dictation-download"
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
              loading={$dictationSetup.importing}
              data-testid="dictation-import"
            >
              <Upload size={12} strokeWidth={2} />
              {$t("onboarding.embeddings.import")}
            </Button>
          {/if}
        </div>
      {/if}

      {#if view.downloadDisabledReasonKey}
        <p class="empty-hint">
          <WifiOff size={12} strokeWidth={2} class="inline-block" />
          {$t(view.downloadDisabledReasonKey)}
        </p>
      {/if}
    </section>
  {/if}

  {#if failure}
    <p class="inline-error" role="alert" data-testid="dictation-error">
      <AlertCircle size={12} />
      {#if failure.kind === "not_offered"}
        {$t("onboarding.dictation_model.error_not_offered", { values: { file: failure.detail ?? "" } })}
      {:else}
        {$t("onboarding.dictation_model.error")}{failure.detail ? ` ${failure.detail}` : ""}
      {/if}
    </p>
  {/if}

  {#if view.showReady}
    <!-- Mounted once the model is in use, so the settings it copies are the
         ones that name it. -->
    <div class="mt-3" data-testid="dictation-test-block">
      <OnboardingDictation open />
    </div>
  {/if}
</div>
