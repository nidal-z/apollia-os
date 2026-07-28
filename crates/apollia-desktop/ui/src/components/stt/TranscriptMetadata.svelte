<script lang="ts">
  /**
   * TranscriptMetadata - Builder-mode engine metadata for a transcript.
   *
   * Collapsed behind a `Disclosure`. Every field maps 1:1 to a real
   * `TranscriptRow` column: nothing here is fabricated, so Builder mode stays a
   * faithful, exhaustive view of what the runtime actually stored.
   */
  import { t } from "svelte-i18n";
  import { Disclosure } from "$lib/components/ui/disclosure";
  import type { TranscriptRow } from "$lib/types";

  let { transcript }: { transcript: TranscriptRow } = $props();

  const rows = $derived<[string, string][]>([
    ["id", transcript.id],
    ["source", transcript.source],
    ["language", transcript.language ?? "-"],
    ["audio_duration_ms", String(transcript.audio_duration_ms)],
    ["processing_time_ms", String(transcript.processing_time_ms)],
    ["model_name", transcript.model_name ?? "-"],
    ["created_at", transcript.created_at],
  ]);
</script>

<div class="mt-3">
  <Disclosure testid="transcript-metadata-{transcript.id}">
    {#snippet summary()}
      <span class="font-mono text-body-xs font-semibold text-muted-foreground">
        {$t("transcriptions.technical_metadata")}
      </span>
    {/snippet}
    {#snippet children()}
      <dl
        class="overflow-hidden rounded-sm border border-[hsl(var(--border-soft))] text-body-xs"
      >
        {#each rows as [key, value], i (key)}
          <div
            class="flex gap-3 px-2.5 py-1.5"
            class:bg-surface-2={i % 2 === 1}
            class:border-t={i > 0}
            class:border-[hsl(var(--border-soft))]={i > 0}
          >
            <dt class="w-36 shrink-0 font-mono text-muted-foreground">{key}</dt>
            <dd class="min-w-0 flex-1 break-all font-mono text-foreground">
              {value}
            </dd>
          </div>
        {/each}
      </dl>
    {/snippet}
  </Disclosure>
</div>
