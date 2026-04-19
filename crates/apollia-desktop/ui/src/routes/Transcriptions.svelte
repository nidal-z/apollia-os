<script lang="ts">
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import { Mic, CheckCircle2, XCircle, Cpu } from "lucide-svelte";
  import { Badge } from "$lib/components/ui/badge";
  import { Skeleton } from "$lib/components/ui/skeleton";
  import EmptyState from "../components/common/EmptyState.svelte";
  import TranscriptCard from "../components/stt/TranscriptCard.svelte";
  import TranscribeFileDialog from "../components/stt/TranscribeFileDialog.svelte";
  import {
    sttStatus,
    transcriptions,
    isRecording,
    refreshSttStatus,
    refreshTranscriptions,
    deleteTranscription,
    listenSttEvents,
  } from "$lib/stores/stt";

  let loading = $state(true);

  onMount(() => {
    const cleanup = listenSttEvents();

    void Promise.allSettled([refreshSttStatus(), refreshTranscriptions()]).then(
      () => {
        loading = false;
      },
    );

    return cleanup;
  });

  async function handleDelete(id: string) {
    try {
      await deleteTranscription(id);
    } catch {
      // deletion failed — state unchanged
    }
  }
</script>

<div class="mx-auto w-full max-w-6xl space-y-6" data-testid="transcriptions-page">
  <!-- Header -->
  <div class="flex items-start justify-between gap-4">
    <div>
      <h1
        class="text-2xl font-semibold tracking-tight text-foreground"
        data-testid="transcriptions-title"
      >
        {$t("transcriptions.title")}
      </h1>
      <p class="mt-1 text-sm text-muted-foreground/60">
        {$t("transcriptions.subtitle")}
      </p>
    </div>
    <TranscribeFileDialog />
  </div>

  <!-- Status banner -->
  {#if $sttStatus}
    <div
      class="glass-card glass-border rounded-xl px-4 py-3 flex flex-wrap items-center gap-x-4 gap-y-1.5 text-sm"
      data-testid="stt-status-banner"
    >
      <div class="flex items-center gap-2">
        {#if $sttStatus.enabled && $sttStatus.model_loaded}
          <CheckCircle2 size={15} strokeWidth={1.75} class="text-success shrink-0" />
          <span class="text-foreground font-medium">{$t("transcriptions.status_active")}</span>
        {:else}
          <XCircle size={15} strokeWidth={1.75} class="text-muted-foreground/50 shrink-0" />
          <span class="text-muted-foreground">{$t("transcriptions.status_inactive")}</span>
        {/if}
      </div>

      {#if $sttStatus.model_loaded}
        <span class="text-muted-foreground/60">&middot;</span>
        <span class="text-muted-foreground/60" data-testid="stt-model-name">
          {$sttStatus.model_name}
        </span>
      {/if}

      {#if $sttStatus.metal_enabled}
        <Badge variant="secondary" class="text-[10px] px-1.5 py-0" data-testid="stt-metal-badge">
          <Cpu size={10} strokeWidth={1.75} class="mr-1" />
          Metal
        </Badge>
      {/if}

      {#if $sttStatus.cuda_enabled}
        <Badge variant="secondary" class="text-[10px] px-1.5 py-0" data-testid="stt-cuda-badge">
          <Cpu size={10} strokeWidth={1.75} class="mr-1" />
          CUDA
        </Badge>
      {/if}

      {#if $isRecording}
        <Badge variant="destructive" class="ml-auto text-[10px] px-1.5 py-0 animate-pulse" data-testid="stt-recording-badge">
          {$t("transcriptions.recording")}
        </Badge>
      {/if}
    </div>
  {/if}

  <!-- Transcription list -->
  {#if loading}
    <div class="grid gap-3">
      {#each { length: 3 } as _}
        <div class="glass-card glass-border rounded-xl p-4">
          <Skeleton class="h-3 w-24" />
          <Skeleton class="mt-3 h-4 w-full" />
          <Skeleton class="mt-2 h-4 w-3/4" />
        </div>
      {/each}
    </div>
  {:else if $transcriptions.length === 0}
    <EmptyState
      icon={Mic}
      title={$t("transcriptions.empty_title")}
      subtitle={$t("transcriptions.empty_subtitle")}
      page="transcriptions"
    />
  {:else}
    <div class="grid gap-3" data-testid="transcriptions-list">
      {#each $transcriptions as transcript (transcript.id)}
        <TranscriptCard {transcript} ondelete={handleDelete} />
      {/each}
    </div>
  {/if}
</div>
