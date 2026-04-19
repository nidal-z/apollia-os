<script lang="ts">
  import { t } from "svelte-i18n";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { FileAudio, AlertCircle } from "lucide-svelte";
  import { Spinner } from "$lib/components/ui/progress";
  import { Button } from "$lib/components/ui/button";
  import { sttStatus, transcribeFile } from "$lib/stores/stt";

  let transcribing = $state(false);
  let errorMessage = $state<string | null>(null);

  const AUDIO_EXTENSIONS = ["wav", "mp3", "ogg", "m4a"];

  async function handleTranscribeFile() {
    errorMessage = null;

    const status = $sttStatus;
    if (status && !status.model_loaded) {
      errorMessage = $t("transcriptions.error_model_not_loaded");
      return;
    }

    const selected = await openDialog({
      filters: [
        {
          name: $t("transcriptions.file_filter_audio"),
          extensions: AUDIO_EXTENSIONS,
        },
      ],
      multiple: false,
    });

    if (!selected) {
      return;
    }

    transcribing = true;
    try {
      await transcribeFile(selected as string);
    } catch (err) {
      errorMessage =
        err instanceof Error ? err.message : String(err);
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

  {#if errorMessage}
    <div
      class="flex items-start gap-2 rounded-lg border border-destructive/30 bg-destructive/5 px-3 py-2 text-xs text-destructive"
      role="alert"
      data-testid="transcribe-file-error"
    >
      <AlertCircle size={14} strokeWidth={1.75} class="mt-0.5 shrink-0" />
      <span>{errorMessage}</span>
    </div>
  {/if}
</div>
