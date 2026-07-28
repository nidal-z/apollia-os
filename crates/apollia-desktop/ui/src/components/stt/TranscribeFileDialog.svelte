<script lang="ts">
  import { t } from "svelte-i18n";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { FileAudio, AlertCircle } from "lucide-svelte";
  import { Spinner } from "$lib/components/ui/progress";
  import { Button } from "$lib/components/ui/button";
  import { Disclosure } from "$lib/components/ui/disclosure";
  import { sttStatus, refreshTranscriptions } from "$lib/stores/stt";
  import { transcribeFile } from "$lib/ipc/stt";
  import { reportError } from "$lib/errors/reportError";
  import type { HumanizedError } from "$lib/errors/humanize";

  let transcribing = $state(false);
  let error = $state<HumanizedError | null>(null);
  const errorDetail = $derived(error?.detail ?? null);

  const AUDIO_EXTENSIONS = ["wav", "mp3", "ogg", "m4a"];

  async function handleTranscribeFile() {
    error = null;

    const status = $sttStatus;
    if (status && !status.model_loaded) {
      error = reportError(new Error($t("transcriptions.error_model_not_loaded")), {
        surface: "inline",
      });
      return;
    }

    const selected = await openDialog({
      filters: [{ name: $t("transcriptions.file_filter_audio"), extensions: AUDIO_EXTENSIONS }],
      multiple: false,
    });
    if (!selected) return;

    transcribing = true;
    try {
      await transcribeFile(selected as string);
      await refreshTranscriptions();
    } catch (err) {
      error = reportError(err, { surface: "inline" });
    } finally {
      transcribing = false;
    }
  }
</script>

<div class="flex flex-col items-start gap-2">
  <Button
    variant="outline"
    size="sm"
    class="gap-2"
    disabled={transcribing}
    onclick={handleTranscribeFile}
    data-testid="transcribe-file-button"
  >
    {#if transcribing}
      <Spinner size={15} />
      {$t("transcriptions.transcribing")}
    {:else}
      <FileAudio size={15} strokeWidth={1.75} />
      {$t("transcriptions.transcribe_file")}
    {/if}
  </Button>

  {#if error}
    <div
      class="flex max-w-md flex-col gap-1.5 rounded-lg border border-destructive/30 bg-destructive/5 px-3 py-2 text-body-xs text-destructive"
      role="alert"
      data-testid="transcribe-file-error"
    >
      <div class="flex items-start gap-2">
        <AlertCircle size={14} strokeWidth={1.75} class="mt-0.5 shrink-0" />
        <span>{error.friendly_message}</span>
      </div>
      {#if errorDetail}
        <Disclosure testid="transcribe-file-error-detail">
          {#snippet summary()}
            <span class="text-body-xs text-muted-foreground">
              {$t("transcriptions.technical_details")}
            </span>
          {/snippet}
          {#snippet children()}
            <pre
              class="whitespace-pre-wrap break-words font-mono text-body-xs text-muted-foreground">{errorDetail}</pre>
          {/snippet}
        </Disclosure>
      {/if}
    </div>
  {/if}
</div>
