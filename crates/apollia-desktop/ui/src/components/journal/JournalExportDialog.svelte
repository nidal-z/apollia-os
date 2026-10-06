<script lang="ts">
  /**
   * JournalExportDialog - how the journal leaves the application: as a CSV
   * file or as a page to print, with the filters in force, the summary of
   * each entry always, and its detail when the user asks for it. The
   * organisation policy may settle the detail: required, the box is checked
   * and frozen; forbidden, the box is not shown.
   */
  import { t } from "svelte-i18n";
  import { AlertTriangle, KeyRound } from "lucide-svelte";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { Checkbox } from "$lib/components/ui/checkbox";
  import { Dialog, DialogFooter } from "$lib/components/ui/dialog";
  import { RadioItem } from "$lib/components/ui/radio";
  import { detailFor, type DetailRule, type ExportFormat } from "./journalExport";

  interface Props {
    open: boolean;
    /** The filters the export takes over, already in words. */
    filters: string;
    rule: DetailRule;
    exporting: boolean;
    /** Why the export just asked could not be made, `null` when none failed. */
    error: string | null;
    onClose: () => void;
    onExport: (format: ExportFormat, wantsDetail: boolean) => void;
  }

  let { open, filters, rule, exporting, error, onClose, onExport }: Props = $props();

  let format = $state<ExportFormat>("csv");
  let wantsDetail = $state(false);

  // Each opening starts from the prudent choice: a file, the summary alone.
  $effect(() => {
    if (open) {
      format = "csv";
      wantsDetail = false;
    }
  });

  const withDetail = $derived(detailFor(rule, wantsDetail));
</script>

<Dialog {open} onclose={onClose} size="sm" title={$t("journal.export.title")} data-testid="journal-export-dialog">
  <div class="space-y-4 text-body-sm">
    <p class="m-0 text-caption text-muted-foreground" data-testid="journal-export-filters">
      {$t("journal.export.intro", { values: { filters } })}
    </p>

    <div class="space-y-1.5">
      <span class="text-caption text-muted-foreground">{$t("journal.export.format")}</span>
      <div class="space-y-2" role="radiogroup" aria-label={$t("journal.export.format")}>
        <RadioItem value="csv" checked={format === "csv"} onchange={() => (format = "csv")} data-testid="journal-export-format-csv">
          {$t("journal.export.format_csv")}
        </RadioItem>
        <RadioItem value="pdf" checked={format === "pdf"} onchange={() => (format = "pdf")} data-testid="journal-export-format-pdf">
          {$t("journal.export.format_pdf")}
        </RadioItem>
      </div>
      {#if format === "pdf"}
        <p class="m-0 text-caption text-muted-foreground" data-testid="journal-export-pdf-hint">
          {$t("journal.export.pdf_hint")}
        </p>
      {/if}
    </div>

    <div class="space-y-2">
      <span class="text-caption text-muted-foreground">{$t("journal.export.content")}</span>
      <p class="m-0">{$t("journal.export.summary_always")}</p>

      {#if rule === "forbidden"}
        <div class="flex items-start gap-2 text-caption text-muted-foreground" data-testid="journal-export-detail-forbidden">
          <KeyRound size={14} strokeWidth={1.75} class="mt-0.5 shrink-0" aria-hidden="true" />
          <p class="m-0 flex-1">{$t("journal.export.detail_forbidden")}</p>
          <Badge variant="outline" size="sm">{$t("journal.export.detail_forbidden_badge")}</Badge>
        </div>
      {:else}
        <div class="flex flex-wrap items-center gap-2">
          <Checkbox
            id="journal-export-detail"
            checked={withDetail}
            disabled={rule === "required"}
            onchange={(checked) => (wantsDetail = checked)}
            data-testid="journal-export-detail"
          />
          <label for="journal-export-detail">{$t("journal.export.include_detail")}</label>
          {#if rule === "required"}
            <Badge variant="outline" size="sm" data-testid="journal-export-detail-required">
              {$t("journal.export.detail_required_badge")}
            </Badge>
          {/if}
        </div>
        {#if rule === "required"}
          <p class="m-0 text-caption text-muted-foreground">{$t("journal.export.detail_required")}</p>
        {/if}
      {/if}

      {#if withDetail}
        <div
          class="flex items-start gap-2 rounded-lg border border-warning/40 bg-warning/5 px-3 py-2.5 text-warning-a11y"
          role="note"
          data-testid="journal-export-personal-data"
        >
          <AlertTriangle size={16} strokeWidth={1.75} class="mt-0.5 shrink-0" aria-hidden="true" />
          <p class="m-0">{$t("journal.export.personal_data")}</p>
        </div>
      {/if}
    </div>

    {#if error !== null}
      <p class="m-0 text-destructive" role="alert" data-testid="journal-export-error">
        {$t("journal.export.failed", { values: { message: error } })}
      </p>
    {/if}
  </div>

  <DialogFooter>
    <Button variant="outline" onclick={onClose} data-testid="journal-export-cancel">
      {$t("journal.export.cancel")}
    </Button>
    <Button
      variant="primary-solid"
      loading={exporting}
      onclick={() => onExport(format, wantsDetail)}
      data-testid="journal-export-confirm"
    >
      {$t("journal.export.confirm")}
    </Button>
  </DialogFooter>
</Dialog>
