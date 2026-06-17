<script lang="ts">
  import { t } from "svelte-i18n";
  import { Mic, FileAudio, Plug, Copy, Trash2 } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { formatRelativeTime, formatDuration } from "$lib/utils";
  import type { TranscriptRow } from "$lib/types";
  import { EntityCard } from "$lib/components/operator";

  let {
    transcript,
    ondelete,
  }: {
    transcript: TranscriptRow;
    ondelete: (id: string) => void;
  } = $props();

  let confirmingDelete = $state(false);
  let copied = $state(false);

  const sourceIcon = $derived.by(() => {
    switch (transcript.source) {
      case "hotkey":
        return Mic;
      case "file":
        return FileAudio;
      case "api":
        return Plug;
    }
  });

  const sourceLabel = $derived.by(() => {
    switch (transcript.source) {
      case "hotkey":
        return $t("transcriptions.source_hotkey");
      case "file":
        return $t("transcriptions.source_file");
      case "api":
        return $t("transcriptions.source_api");
    }
  });

  async function copyText() {
    try {
      await navigator.clipboard.writeText(transcript.full_text);
      copied = true;
      setTimeout(() => {
        copied = false;
      }, 1500);
    } catch {
      // clipboard API unavailable
    }
  }

  function handleDelete() {
    if (!confirmingDelete) {
      confirmingDelete = true;
      setTimeout(() => {
        confirmingDelete = false;
      }, 3000);
      return;
    }
    confirmingDelete = false;
    ondelete(transcript.id);
  }
</script>

<EntityCard
  iconTone="info"
  title={sourceLabel}
  data-testid="transcript-card-{transcript.id}"
>
  {#snippet icon()}
    <svelte:component this={sourceIcon} size={16} strokeWidth={1.75} />
  {/snippet}

  {#snippet body()}
    <div class="flex flex-wrap items-center gap-2 text-xs text-muted-foreground/60">
      <span data-testid="transcript-source">{sourceLabel}</span>
      <span>&middot;</span>
      <span data-testid="transcript-time">{formatRelativeTime(transcript.created_at)}</span>
      <span>&middot;</span>
      <span data-testid="transcript-duration">{formatDuration(transcript.audio_duration_ms)}</span>
      {#if transcript.language}
        <span>&middot;</span>
        <span data-testid="transcript-language">{transcript.language}</span>
      {/if}
    </div>
    <p
      class="mt-2 text-sm text-foreground leading-relaxed"
      data-testid="transcript-text"
    >
      {transcript.full_text}
    </p>
  {/snippet}

  {#snippet actions()}
    <Button
      variant="ghost"
      size="sm"
      class="h-7 gap-1.5 text-xs text-muted-foreground"
      onclick={copyText}
      data-testid="transcript-copy-{transcript.id}"
    >
      <Copy size={13} strokeWidth={1.75} />
      {copied ? $t("transcriptions.copied") : $t("transcriptions.copy")}
    </Button>
    <Button
      variant={confirmingDelete ? "destructive" : "ghost"}
      size="sm"
      class="h-7 gap-1.5 text-xs {confirmingDelete ? '' : 'text-muted-foreground'}"
      onclick={handleDelete}
      data-testid="transcript-delete-{transcript.id}"
    >
      <Trash2 size={13} strokeWidth={1.75} />
      {confirmingDelete ? $t("common.confirm") : $t("common.delete")}
    </Button>
  {/snippet}
</EntityCard>
