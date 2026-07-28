<script lang="ts">
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import { flip } from "svelte/animate";
  import { fly } from "svelte/transition";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { Mic, Cpu, Settings2, CheckCircle2, XCircle } from "lucide-svelte";
  import { Badge } from "$lib/components/ui/badge";
  import { Card } from "$lib/components/ui/card";
  import { Button } from "$lib/components/ui/button";
  import {
    PageLayout,
    ErrorBanner,
    SkeletonList,
    StatusDot,
  } from "$lib/components/operator";
  import { EmptyState } from "$lib/components/layout";
  import { listNavigation } from "$lib/components/operator/listNavigation";
  import { listFlip, rowIn, rowOut } from "$lib/design/listMotion";
  import { reportError } from "$lib/errors/reportError";
  import type { HumanizedError } from "$lib/errors/humanize";
  import { navigateToSettings } from "$lib/router";
  import { getSttStatus, listTranscriptions, transcribeFile } from "$lib/ipc/stt";
  import {
    sttStatus,
    transcriptions,
    isRecording,
    refreshTranscriptions,
    deleteTranscription,
    listenSttEvents,
  } from "$lib/stores/stt";
  import TranscriptCard from "../components/stt/TranscriptCard.svelte";
  import TranscribeFileDialog from "../components/stt/TranscribeFileDialog.svelte";
  import TranscribingCard from "../components/stt/TranscribingCard.svelte";

  const AUDIO_EXTENSIONS = ["wav", "mp3", "ogg", "m4a"];

  let loadState = $state<"loading" | "error" | "ready">("loading");
  let loadError = $state<HumanizedError | null>(null);

  async function load(showLoading = false) {
    if (showLoading) loadState = "loading";
    // The engine status and the history are independent: get_stt_status errors
    // whenever dictation is off or no model is loaded, which is a legitimate
    // state the banner already renders. Only the history failing is a real
    // page error, so the two are not awaited together.
    const status = await getSttStatus().catch(() => null);
    if (status) sttStatus.set(status);
    try {
      transcriptions.set(await listTranscriptions());
      loadError = null;
      loadState = "ready";
    } catch (err) {
      loadError = reportError(err, { surface: "inline" });
      loadState = "error";
    }
  }

  onMount(() => {
    const cleanup = listenSttEvents();
    void load(true);
    return cleanup;
  });

  async function handleDelete(id: string) {
    try {
      await deleteTranscription(id);
    } catch (err) {
      reportError(err, { surface: "toast", testid: "transcript-delete-error" });
    }
  }

  async function pickAndTranscribe() {
    const selected = await openDialog({
      filters: [{ name: $t("transcriptions.file_filter_audio"), extensions: AUDIO_EXTENSIONS }],
      multiple: false,
    });
    if (!selected) return;
    try {
      await transcribeFile(selected as string);
      await refreshTranscriptions();
    } catch (err) {
      reportError(err, { surface: "toast", testid: "transcribe-file-error" });
    }
  }
</script>

<PageLayout
  kicker={$t("transcriptions.kicker")}
  title={$t("transcriptions.title")}
  subtitle={$t("transcriptions.subtitle")}
  data-testid="transcriptions-page"
>
  {#snippet actions()}
    <TranscribeFileDialog />
  {/snippet}

  <div class="space-y-6 px-8 pb-8 pt-6">
    {#if $sttStatus}
      <Card
        class="flex flex-wrap items-center gap-x-4 gap-y-1.5 px-4 py-3 text-body-sm"
        data-testid="stt-status-banner"
      >
        <div class="flex items-center gap-2">
          {#if $sttStatus.enabled && $sttStatus.model_loaded}
            <CheckCircle2 size={15} strokeWidth={1.75} class="shrink-0 text-success" />
            <span class="font-medium text-foreground">{$t("transcriptions.status_active")}</span>
          {:else}
            <XCircle size={15} strokeWidth={1.75} class="shrink-0 text-muted-foreground/50" />
            <span class="text-muted-foreground">{$t("transcriptions.status_inactive")}</span>
          {/if}
        </div>

        {#if $sttStatus.model_loaded}
          <span class="text-muted-foreground/60" aria-hidden="true">&middot;</span>
          <span class="font-mono text-body-xs text-muted-foreground" data-testid="stt-model-name">
            {$sttStatus.model_name}
          </span>
        {/if}

        {#if $sttStatus.metal_enabled}
          <Badge variant="secondary" class="gap-1" data-testid="stt-metal-badge">
            <Cpu size={10} strokeWidth={1.75} />Metal
          </Badge>
        {/if}
        {#if $sttStatus.cuda_enabled}
          <Badge variant="secondary" class="gap-1" data-testid="stt-cuda-badge">
            <Cpu size={10} strokeWidth={1.75} />CUDA
          </Badge>
        {/if}

        {#if $isRecording}
          <span
            class="inline-flex items-center gap-1.5 text-body-xs font-medium text-destructive"
            data-testid="stt-recording-badge"
          >
            <StatusDot color="hsl(var(--destructive))" glow />
            {$t("transcriptions.recording")}
          </span>
        {/if}

        <Button
          variant="ghost"
          size="sm"
          class="ml-auto h-7 gap-1.5 text-body-xs text-primary"
          onclick={() => navigateToSettings("stt")}
          data-testid="stt-configure-link"
        >
          <Settings2 size={13} strokeWidth={1.75} />
          {$t("transcriptions.configure_engine")}
        </Button>
      </Card>
    {/if}

    {#if $sttStatus && !$sttStatus.model_loaded}
      <ErrorBanner
        tone="warning"
        onretry={() => load(true)}
        retryLabel={$t("common.retry")}
        data-testid="stt-model-not-loaded"
      >
        <div class="flex flex-wrap items-center gap-x-3 gap-y-1">
          <span>{$t("transcriptions.error_model_not_loaded")}</span>
          <button
            type="button"
            class="font-medium underline underline-offset-2 outline-none focus-visible:no-underline"
            onclick={() => navigateToSettings("stt")}
            data-testid="stt-open-settings"
          >
            {$t("transcriptions.error_open_settings")}
          </button>
        </div>
      </ErrorBanner>
    {/if}

    {#if $isRecording}
      <TranscribingCard />
    {/if}

    {#if loadState === "loading"}
      <SkeletonList count={3} />
    {:else if loadState === "error" && loadError}
      <ErrorBanner
        message={loadError.friendly_message}
        onretry={() => load(true)}
        retryLabel={$t("common.retry")}
        data-testid="transcriptions-load-error"
      />
    {:else if $transcriptions.length === 0 && !$isRecording}
      <EmptyState
        icon={Mic}
        title={$t("transcriptions.empty_title")}
        description={$t("transcriptions.empty_subtitle")}
        primaryLabel={$t("transcriptions.transcribe_file")}
        primaryAction={pickAndTranscribe}
        secondaryLabel={$t("transcriptions.empty_configure_hotkey")}
        secondaryAction={() => navigateToSettings("stt")}
        page="transcriptions"
      />
    {:else}
      <div
        class="grid gap-3"
        data-testid="transcriptions-list"
        use:listNavigation={{ rowSelector: "[data-nav-card]" }}
      >
        {#each $transcriptions as transcript (transcript.id)}
          <div animate:flip={listFlip()} in:fly={rowIn()} out:fly={rowOut()}>
            <TranscriptCard {transcript} ondelete={handleDelete} />
          </div>
        {/each}
      </div>
    {/if}
  </div>
</PageLayout>
