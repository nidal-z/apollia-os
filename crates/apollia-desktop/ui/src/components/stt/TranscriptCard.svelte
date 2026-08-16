<script lang="ts">
  /**
   * TranscriptCard - one transcript in the history list.
   *
   * Clean read layout (source, meta, decorative waveform, text) on the canon
   * `EntityCard` shell, wrapped in a right-click `ContextMenu`. The card is a
   * roving-tabindex list row: it is focusable and answers C / E / Delete when
   * focused. Builder mode reveals the raw engine metadata.
   */
  import { t, locale } from "svelte-i18n";
  import { Mic, FileAudio, Plug, Copy, Download, Trash2 } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { EntityCard } from "$lib/components/operator";
  import { ContextMenu, type ActionMenuItem } from "$lib/components/ui/context-menu";
  import { uiMode } from "$lib/stores/mode";
  import { formatRelativeTime, formatDuration } from "$lib/utils";
  import type { TranscriptRow } from "$lib/types";
  import TranscriptWaveform from "./TranscriptWaveform.svelte";
  import TranscriptMetadata from "./TranscriptMetadata.svelte";

  let {
    transcript,
    ondelete,
  }: {
    transcript: TranscriptRow;
    ondelete: (id: string) => void;
  } = $props();

  let confirmingDelete = $state(false);
  let copied = $state(false);
  let expanded = $state(false);
  let canToggle = $state(false);
  let textEl = $state<HTMLParagraphElement | null>(null);

  const ICONS = { hotkey: Mic, file: FileAudio, api: Plug } as const;
  const TONES = { hotkey: "info", file: "primary", api: "success" } as const;

  const sourceLabel = $derived(
    $t(`transcriptions.source_${transcript.source}`) as string,
  );

  $effect(() => {
    // Measure overflow only while clamped; keep the toggle once it appears.
    void transcript.full_text;
    if (expanded) return;
    const el = textEl;
    if (el) canToggle = el.scrollHeight > el.clientHeight + 1;
  });

  async function copyText() {
    try {
      await navigator.clipboard.writeText(transcript.full_text);
      copied = true;
      setTimeout(() => (copied = false), 1500);
    } catch {
      // clipboard API unavailable
    }
  }

  function exportText() {
    // Browser-native download - works inside the Tauri webview without an
    // extra fs plugin (same pattern as the chat session export).
    const blob = new Blob([transcript.full_text], { type: "text/plain" });
    const url = URL.createObjectURL(blob);
    const anchor = document.createElement("a");
    anchor.href = url;
    anchor.download = `transcript-${transcript.id}.txt`;
    document.body.appendChild(anchor);
    anchor.click();
    anchor.remove();
    URL.revokeObjectURL(url);
  }

  function handleDelete() {
    if (!confirmingDelete) {
      confirmingDelete = true;
      setTimeout(() => (confirmingDelete = false), 3000);
      return;
    }
    confirmingDelete = false;
    ondelete(transcript.id);
  }

  function onKeydown(event: KeyboardEvent) {
    if (event.defaultPrevented || event.metaKey || event.ctrlKey || event.altKey) {
      return;
    }
    if (event.target !== event.currentTarget) return;
    const key = event.key.toLowerCase();
    if (key === "c") {
      event.preventDefault();
      void copyText();
    } else if (key === "e") {
      event.preventDefault();
      exportText();
    } else if (event.key === "Delete" || event.key === "Backspace") {
      event.preventDefault();
      handleDelete();
    }
  }

  const menuItems = $derived<ActionMenuItem[]>([
    { id: "copy", label: $t("transcriptions.ctx_copy"), icon: Copy, onclick: () => void copyText() },
    { id: "export", label: $t("transcriptions.ctx_export"), icon: Download, onclick: exportText },
    {
      id: "delete",
      label: $t("common.delete"),
      icon: Trash2,
      variant: "destructive",
      onclick: handleDelete,
    },
  ]);
</script>

<ContextMenu items={menuItems} data-testid="transcript-menu-{transcript.id}">
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div
    role="article"
    tabindex="-1"
    data-nav-card
    data-testid="transcript-card-{transcript.id}"
    aria-label={sourceLabel}
    onkeydown={onKeydown}
    class="rounded-xl outline-none focus-visible:ring-2 focus-visible:ring-primary/40"
  >
    <EntityCard iconTone={TONES[transcript.source]} title={sourceLabel}>
      {#snippet icon()}
        {@const SourceIcon = ICONS[transcript.source]}
        <SourceIcon size={16} strokeWidth={1.75} aria-hidden="true" />
      {/snippet}

      {#snippet body()}
        <div class="flex flex-wrap items-center gap-2 text-body-xs text-muted-foreground/70">
          <span data-testid="transcript-time"
            >{formatRelativeTime(transcript.created_at, $locale ?? "en")}</span
          >
          <span aria-hidden="true">&middot;</span>
          <span class="tabular-nums" data-testid="transcript-duration">
            {formatDuration(transcript.audio_duration_ms)}
          </span>
          {#if transcript.language}
            <span
              class="rounded border border-[hsl(var(--border-soft))] bg-surface-2 px-1.5 font-mono text-overline uppercase text-muted-foreground"
              data-testid="transcript-language"
            >
              {transcript.language}
            </span>
          {/if}
        </div>

        <div class="mt-2.5 flex items-center gap-3">
          <TranscriptWaveform seed={transcript.id} />
        </div>

        <p
          bind:this={textEl}
          class="mt-3 whitespace-pre-wrap text-body-md leading-relaxed text-foreground"
          class:line-clamp-3={!expanded}
          data-testid="transcript-text"
        >
          {transcript.full_text}
        </p>
        {#if canToggle}
          <button
            type="button"
            class="mt-1.5 text-body-sm font-medium text-primary outline-none hover:underline focus-visible:underline"
            onclick={() => (expanded = !expanded)}
            data-testid="transcript-toggle-{transcript.id}"
          >
            {expanded ? $t("transcriptions.show_less") : $t("transcriptions.show_more")}
          </button>
        {/if}

        {#if $uiMode === "builder"}
          <TranscriptMetadata {transcript} />
        {/if}
      {/snippet}

      {#snippet actions()}
        <Button
          variant="ghost"
          size="sm"
          class="h-7 gap-1.5 text-body-xs {copied ? 'text-success' : 'text-muted-foreground'}"
          onclick={copyText}
          data-testid="transcript-copy-{transcript.id}"
        >
          <Copy size={13} strokeWidth={1.75} />
          {copied ? $t("transcriptions.copied") : $t("transcriptions.copy")}
        </Button>
        <Button
          variant="ghost"
          size="sm"
          class="h-7 gap-1.5 text-body-xs text-muted-foreground"
          onclick={exportText}
          data-testid="transcript-export-{transcript.id}"
        >
          <Download size={13} strokeWidth={1.75} />
          {$t("transcriptions.export")}
        </Button>
        <Button
          variant={confirmingDelete ? "destructive" : "ghost"}
          size="sm"
          class="h-7 gap-1.5 text-body-xs {confirmingDelete ? '' : 'text-muted-foreground'}"
          onclick={handleDelete}
          data-testid="transcript-delete-{transcript.id}"
        >
          <Trash2 size={13} strokeWidth={1.75} />
          {confirmingDelete ? $t("common.confirm") : $t("common.delete")}
        </Button>
      {/snippet}
    </EntityCard>
  </div>
</ContextMenu>
