<script lang="ts">
  /**
   * WorkbookPreview - what a batch on a workbook or a CSV file would read,
   * shown in the launch dialog before anything runs: the sheet and the header
   * row proposed, both open to change, the first rows of the sheet, one
   * checkbox per column and the number of items.
   *
   * The decisions live in `workbookPreview.ts`; the dialog holds the preview
   * and the selection, and asks the runtime again when the sheet or the
   * header row changes.
   */
  import { t } from "svelte-i18n";
  import { Banner } from "$lib/components/ui/banner";
  import { Checkbox } from "$lib/components/ui/checkbox";
  import { Select } from "$lib/components/ui/select";
  import type { SourcePreview } from "$lib/ipc/batches";
  import {
    cellOf,
    checkedColumns,
    formattingLost,
    headerRowChoices,
    isChecked,
    rowRole,
    withColumn,
    withHeaderRow,
    withSheet,
    type PreviewSelection,
  } from "./workbookPreview";

  interface Props {
    /** The preview the runtime answered; `null` before the first answer. */
    preview: SourcePreview | null;
    selection: PreviewSelection;
    /** The runtime is reading the file. */
    loading: boolean;
    /** Why the file could not be read. */
    error: string | null;
    onselection: (selection: PreviewSelection) => void;
  }

  let { preview, selection, loading, error, onselection }: Props = $props();

  const ROW_CLASSES = {
    above: "text-muted-foreground",
    header: "bg-muted font-medium text-foreground",
    item: "text-foreground",
  } as const;

  const checked = $derived(preview ? checkedColumns(preview, selection).length : 0);

  function pickSheet(name: string): void {
    if (preview === null || name === preview.sheet) return;
    onselection(withSheet(name));
  }

  function pickHeaderRow(value: string): void {
    const row = Number.parseInt(value, 10);
    if (preview === null || !Number.isInteger(row) || row === preview.headerRow) return;
    onselection(withHeaderRow(preview, selection, row));
  }
</script>

{#if error || loading || preview}
  <section class="space-y-2" data-testid="batch-preview" aria-busy={loading}>
    <h3 class="m-0 text-body-xs font-medium text-muted-foreground">{$t("batch_launch.preview.title")}</h3>
    {#if error}
      <p class="m-0 text-caption text-destructive" data-testid="batch-preview-error">
        {$t("batch_launch.preview.unreadable", { values: { reason: error } })}
      </p>
    {:else if preview === null}
      <p class="m-0 text-caption text-muted-foreground" data-testid="batch-preview-loading">
        {$t("batch_launch.preview.loading")}
      </p>
    {:else}
      <div class="grid grid-cols-2 gap-2">
        <label class="block min-w-0 space-y-1">
          <span class="block text-caption text-muted-foreground">{$t("batch_launch.preview.sheet")}</span>
          <Select
            value={preview.sheet ?? ""}
            size="sm"
            disabled={loading}
            onchange={(event) => pickSheet(event.currentTarget.value)}
            data-testid="batch-preview-sheet"
          >
            {#each preview.sheets as sheet (sheet.name)}
              <option value={sheet.name}>
                {sheet.name === preview.proposedSheet
                  ? $t("batch_launch.preview.proposed", { values: { name: sheet.name } })
                  : sheet.name}
              </option>
            {/each}
          </Select>
        </label>
        <label class="block min-w-0 space-y-1">
          <span class="block text-caption text-muted-foreground">{$t("batch_launch.preview.header_row")}</span>
          <Select
            value={preview.headerRow === null ? "" : String(preview.headerRow)}
            size="sm"
            disabled={loading}
            onchange={(event) => pickHeaderRow(event.currentTarget.value)}
            data-testid="batch-preview-header-row"
          >
            {#if preview.headerRow === null}
              <option value="">{$t("batch_launch.preview.no_header")}</option>
            {/if}
            {#each headerRowChoices(preview) as row (row)}
              <option value={String(row)}>
                {row === preview.proposedHeaderRow
                  ? $t("batch_launch.preview.row_proposed", { values: { row } })
                  : $t("batch_launch.preview.row", { values: { row } })}
              </option>
            {/each}
          </Select>
        </label>
      </div>
      <p class="m-0 text-caption text-muted-foreground" data-testid="batch-preview-sheets">
        {$t("batch_launch.preview.sheets", { values: { count: preview.sheets.length } })}
      </p>
      {#if formattingLost(preview)}
        <Banner variant="warning" surface="card" data-testid="batch-preview-formatting">
          {$t("batch_launch.preview.formatting_not_kept", { values: { format: preview.format } })}
        </Banner>
      {/if}
      {#if preview.columns.length === 0}
        <p class="m-0 text-caption text-muted-foreground" data-testid="batch-preview-empty">
          {$t("batch_launch.preview.empty")}
        </p>
      {:else}
        <div class="max-h-64 overflow-auto rounded-md border border-border bg-card" data-testid="batch-preview-table">
          <table class="w-full border-collapse text-left text-caption">
            <thead class="sticky top-0 bg-card">
              <tr class="border-b border-border">
                <th scope="col" class="px-2 py-1.5 font-medium text-muted-foreground">
                  {$t("batch_launch.preview.row_number")}
                </th>
                {#each preview.columns as column (column.letter)}
                  {@const on = isChecked(selection, column.letter)}
                  <th scope="col" class="px-2 py-1.5 font-medium" data-testid="batch-preview-column" data-checked={on}>
                    <label class="flex cursor-pointer select-none items-center gap-1.5 whitespace-nowrap">
                      <Checkbox
                        checked={on}
                        onchange={(next) => onselection(withColumn(selection, column.letter, next))}
                        data-testid="batch-preview-column-check"
                      />
                      <span class={on ? "text-foreground" : "text-muted-foreground line-through"}>{column.name}</span>
                    </label>
                  </th>
                {/each}
              </tr>
            </thead>
            <tbody>
              {#each preview.rows as row (row.row)}
                {@const role = rowRole(preview, row)}
                <tr class="border-b border-border last:border-b-0 {ROW_CLASSES[role]}" data-testid="batch-preview-row" data-role={role}>
                  <td class="px-2 py-1 tabular-nums text-muted-foreground">{row.row}</td>
                  {#each preview.columns as column (column.letter)}
                    <td
                      class={isChecked(selection, column.letter)
                        ? "max-w-48 truncate px-2 py-1"
                        : "max-w-48 truncate px-2 py-1 opacity-40"}
                      title={cellOf(row, column.letter)}
                    >
                      {cellOf(row, column.letter)}
                    </td>
                  {/each}
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/if}
      <p class="m-0 text-caption text-muted-foreground" data-testid="batch-preview-count">
        {$t("batch_launch.preview.rows_shown", { values: { count: preview.rows.length } })}
        {$t("batch_launch.preview.items", { values: { count: preview.estimatedItems } })}
        {$t("batch_launch.preview.columns_sent", { values: { checked, total: preview.columns.length } })}
      </p>
    {/if}
  </section>
{/if}
