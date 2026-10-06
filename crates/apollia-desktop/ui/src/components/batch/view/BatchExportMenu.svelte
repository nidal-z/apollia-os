<script lang="ts">
  /**
   * BatchExportMenu - the "Export" command of a batch and the menu it opens:
   * the three formats, each with what it is for.
   */
  import { t } from "svelte-i18n";
  import { ActionMenu } from "$lib/components/ui/action-menu";
  import { Button } from "$lib/components/ui/button";
  import type { BatchExportFormat } from "$lib/ipc/batches";
  import { EXPORT_FORMATS } from "./batchView";

  interface Props {
    disabled?: boolean;
    loading?: boolean;
    onExport: (format: BatchExportFormat) => void;
  }

  let { disabled = false, loading = false, onExport }: Props = $props();

  let open = $state(false);
</script>

<ActionMenu bind:open align="end" class="min-w-[18rem]">
  {#snippet triggerSlot(triggerProps)}
    <Button {...triggerProps} variant="outline" size="sm" {disabled} {loading} data-testid="batch-export">
      {$t("batch_view.command.export")}
    </Button>
  {/snippet}
  {#snippet body({ close })}
    <ul class="m-0 flex list-none flex-col gap-0.5 p-0" role="menu" data-testid="batch-export-menu">
      {#each EXPORT_FORMATS as format (format)}
        <li role="none">
          <button
            type="button"
            role="menuitem"
            class="flex w-full flex-col gap-0.5 rounded-sm px-2 py-1.5 text-left transition-colors hover:bg-muted focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
            onclick={() => {
              close();
              onExport(format);
            }}
            data-testid={`batch-export-${format}`}
          >
            <span class="text-body-sm font-medium text-foreground">{$t(`batch_view.export.${format}`)}</span>
            <span class="text-caption text-muted-foreground">{$t(`batch_view.export.${format}_use`)}</span>
          </button>
        </li>
      {/each}
    </ul>
  {/snippet}
</ActionMenu>
