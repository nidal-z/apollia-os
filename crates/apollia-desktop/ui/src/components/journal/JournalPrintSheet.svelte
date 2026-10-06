<script lang="ts">
  /**
   * JournalPrintSheet - the export of the journal laid out as a page. The
   * print dialog of the system prints it, and saving it as a PDF is a choice
   * of that dialog: no file is produced here. Shown in the light theme while
   * it is open, since paper is light whatever the theme of the window.
   */
  import { locale, t } from "svelte-i18n";
  import { Printer } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { Dialog } from "$lib/components/ui/dialog";
  import type { JournalExportDocument } from "$lib/ipc/journal";
  import { printRows, type PrintCell } from "./journalExport";
  import "./journal-print.css";

  interface Props {
    /** The export to lay out, `null` when no preview is shown. */
    document: JournalExportDocument | null;
    /** The filters the export took over, already in words. */
    filters: string;
    agentName: (agent: string | null) => string;
    onPrint: () => void;
    onClose: () => void;
  }

  let { document: exported, filters, agentName, onPrint, onClose }: Props = $props();

  const rows = $derived(exported === null ? [] : printRows(exported, agentName));
  const printedAt = $derived(exported === null ? null : new Date());

  // Paper is light: the dark theme steps aside while the page is shown.
  $effect(() => {
    if (exported === null) return;
    const root = window.document.documentElement;
    const wasDark = root.classList.contains("dark");
    root.classList.remove("dark");
    return () => {
      if (wasDark) root.classList.add("dark");
    };
  });

  function instant(value: string | Date): string {
    const date = typeof value === "string" ? new Date(value) : value;
    if (Number.isNaN(date.getTime())) return typeof value === "string" ? value : "";
    return date.toLocaleString($locale ?? "en", {
      day: "numeric",
      month: "short",
      year: "numeric",
      hour: "2-digit",
      minute: "2-digit",
    });
  }

  function cell(value: PrintCell): string {
    if (value === null) return "";
    return "key" in value ? $t(value.key) : value.text;
  }
</script>

<Dialog
  open={exported !== null}
  onclose={onClose}
  size="xl"
  label={$t("journal.title")}
  hideClose
  data-testid="journal-print"
>
  {#if exported !== null}
    <div class="journal-print-page space-y-4 text-body-sm">
      <div class="journal-print-controls flex flex-wrap items-center justify-between gap-3">
        <p class="m-0 text-caption text-muted-foreground">{$t("journal.print.hint")}</p>
        <div class="flex flex-wrap items-center gap-2">
          <Button variant="outline" onclick={onClose} data-testid="journal-print-close">
            {$t("journal.print.close")}
          </Button>
          <Button variant="primary-solid" onclick={onPrint} data-testid="journal-print-confirm">
            {#snippet icon()}
              <Printer size={14} strokeWidth={1.75} />
            {/snippet}
            {$t("journal.print.print")}
          </Button>
        </div>
      </div>

      <header class="space-y-1 border-b border-border pb-3">
        <h2 class="m-0 text-body-md font-medium" data-testid="journal-print-title">{$t("journal.title")}</h2>
        <p class="m-0 text-caption text-muted-foreground" data-testid="journal-print-summary">
          {$t("journal.print.summary", {
            values: { date: printedAt === null ? "" : instant(printedAt), filters, count: exported.count },
          })}
        </p>
        {#if exported.detail}
          <p class="m-0 text-caption text-warning-a11y" data-testid="journal-print-personal-data">
            {$t("journal.print.detail_note")}
          </p>
        {/if}
      </header>

      <table class="w-full border-collapse text-left text-caption" data-testid="journal-print-table">
        <thead>
          <tr class="border-b border-border text-muted-foreground">
            <th class="py-1.5 pr-3 font-normal">{$t("journal.detail.when")}</th>
            <th class="py-1.5 pr-3 font-normal">{$t("journal.detail.agent")}</th>
            <th class="py-1.5 pr-3 font-normal">{$t("journal.detail.action")}</th>
            <th class="py-1.5 pr-3 font-normal">{$t("journal.detail.family")}</th>
            <th class="py-1.5 pr-3 font-normal">{$t("journal.detail.result")}</th>
            <th class="py-1.5 font-normal">{$t("journal.print.place")}</th>
          </tr>
        </thead>
        <tbody>
          {#each rows as row (row.id)}
            <tr class="border-b border-border align-top" data-testid="journal-print-row">
              <td class="py-1.5 pr-3 whitespace-nowrap">{instant(row.ts)}</td>
              <td class="py-1.5 pr-3">{row.agent ?? ""}</td>
              <td class="py-1.5 pr-3">
                <div>{"key" in row.what ? $t(row.what.key, { values: row.what.values }) : row.what.text}</div>
                {#if row.target !== null}
                  <div class="text-muted-foreground">{$t("journal.target", { values: { target: row.target } })}</div>
                {/if}
                {#if row.tool !== null}
                  <div class="font-mono text-muted-foreground">{row.tool}</div>
                {/if}
              </td>
              <td class="py-1.5 pr-3">{cell(row.family) || cell(row.nature)}</td>
              <td class="py-1.5 pr-3">{cell(row.outcome)}</td>
              <td class="py-1.5 font-mono text-muted-foreground">
                <div class="break-all">{$t("journal.print.place_value", { values: { run: row.runId, seq: row.seq } })}</div>
                <div class="break-all">{row.hash}</div>
              </td>
            </tr>
            {#if row.detail !== null}
              <tr class="border-b border-border align-top" data-testid="journal-print-detail">
                <td></td>
                <td colspan="5" class="space-y-1.5 py-1.5">
                  {#if row.detail.arguments !== null}
                    <div class="text-muted-foreground">{$t("journal.detail.parameters")}</div>
                    <pre class="m-0 whitespace-pre-wrap break-all font-mono">{row.detail.arguments}</pre>
                  {/if}
                  {#if row.detail.output !== null}
                    <div class="text-muted-foreground">{$t("journal.detail.output")}</div>
                    <pre class="m-0 whitespace-pre-wrap break-all font-mono">{row.detail.output}</pre>
                  {/if}
                  <div class="text-muted-foreground">{$t("journal.detail.record")}</div>
                  <pre class="m-0 whitespace-pre-wrap break-all font-mono">{row.detail.payload}</pre>
                </td>
              </tr>
            {/if}
          {/each}
        </tbody>
      </table>

      {#if rows.length === 0}
        <p class="m-0 text-muted-foreground" data-testid="journal-print-empty">{$t("journal.print.empty")}</p>
      {/if}
    </div>
  {/if}
</Dialog>
