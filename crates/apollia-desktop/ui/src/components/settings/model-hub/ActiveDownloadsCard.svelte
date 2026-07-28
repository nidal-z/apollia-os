<script lang="ts">
  /**
   * ActiveDownloadsCard - the live download queue.
   *
   * One row per in-flight file, fed by the `model-download-progress` event.
   * A signature-gradient bar tracks percentage; before the first byte
   * (`total_bytes === null`) the bar reads as indeterminate.
   */
  import { t } from "svelte-i18n";
  import { flip } from "svelte/animate";
  import { fly } from "svelte/transition";
  import { Download, X } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import SettingsSection from "../SettingsSection.svelte";
  import { listFlip, rowIn, rowOut } from "$lib/design/listMotion";
  import type { DownloadProgress } from "$lib/ipc/modelHub";
  import { formatProgress, formatSpeed, progressPercent } from "./helpers";

  interface Props {
    downloads: DownloadProgress[];
    onCancel: (id: string) => void;
  }

  let { downloads, onCancel }: Props = $props();

  function fileName(path: string): string {
    return path.split("/").pop() ?? path;
  }
</script>

<SettingsSection title={$t("settings.model_hub.downloads.active")} data-testid="active-downloads-section">
  {#snippet icon()}<Download size={15} strokeWidth={1.75} class="text-primary" />{/snippet}

  <div class="flex flex-col gap-3">
    {#each downloads as dl (dl.id)}
      {@const indeterminate = dl.total_bytes === null && dl.status === "in_progress"}
      <div
        class="flex items-center gap-3"
        animate:flip={listFlip()}
        in:fly={rowIn()}
        out:fly={rowOut()}
        data-testid="active-download-{dl.id}"
      >
        <div class="min-w-0 flex-1">
          <div class="truncate font-mono text-caption text-muted-foreground">{fileName(dl.dest_path)}</div>
          <div class="mt-1.5 h-1.5 w-full overflow-hidden rounded-full bg-muted">
            {#if indeterminate}
              <div class="h-full w-2/5 animate-pulse rounded-full bg-primary-gradient"></div>
            {:else}
              <div
                class="h-full rounded-full bg-primary-gradient transition-[width] duration-slow"
                class:bg-destructive={dl.status === "failed"}
                style="width: {progressPercent(dl)}%"
              ></div>
            {/if}
          </div>
        </div>
        <div class="w-24 text-right text-caption tabular-nums text-muted-foreground">
          {#if dl.status === "failed"}
            <span class="text-danger-a11y">{$t("settings.model_hub.downloads.failed")}</span>
          {:else}
            {formatProgress(dl)} · {formatSpeed(dl.speed_bps)}
          {/if}
        </div>
        <Button
          variant="ghost"
          size="icon-sm"
          onclick={() => onCancel(dl.id)}
          class="text-muted-foreground hover:text-destructive"
          aria-label={$t("settings.model_hub.downloads.cancel_aria")}
          data-testid="model-download-cancel-{dl.id}"
        >
          <X size={14} />
        </Button>
      </div>
    {/each}
  </div>
</SettingsSection>
