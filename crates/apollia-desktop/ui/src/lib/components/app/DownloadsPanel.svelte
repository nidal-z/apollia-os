<script lang="ts">
  /**
   * DownloadsPanel - the downloads icon of the top bar and the panel it opens.
   *
   * The icon carries a counter while a download is in progress. The panel lists
   * the downloads in three groups: in progress (name, progress, amount, speed,
   * Cancel), finished (completed or cancelled) and failed (readable reason).
   * It reads the `downloads` store, which listens to the single progress event.
   */
  import { t } from "svelte-i18n";
  import { Download, CheckCircle2, AlertTriangle, CircleSlash, X } from "lucide-svelte";
  import { Popover } from "$lib/components/ui/popover";
  import { ProgressBar } from "$lib/components/ui/progress";
  import { Button } from "$lib/components/ui/button";
  import { reportError } from "$lib/errors/reportError";
  import {
    activeDownloadCount,
    cancelDownload,
    downloadGroups,
    downloadPercent,
    downloadsPanelOpen,
    failureReason,
    formatBytes,
    formatSpeed,
    listenDownloads,
    type DownloadEntry,
    type FailureReason,
  } from "$lib/stores/downloads";

  $effect(() => {
    let unlisten: (() => void) | null = null;
    let disposed = false;
    listenDownloads()
      .then((fn) => {
        if (disposed) fn();
        else unlisten = fn;
      })
      .catch((err) => reportError(err, { surface: "inline" }));
    return () => {
      disposed = true;
      unlisten?.();
    };
  });

  const groups = $derived($downloadGroups);
  const count = $derived($activeDownloadCount);
  const empty = $derived(
    groups.active.length + groups.finished.length + groups.failed.length === 0,
  );
  const buttonLabel = $derived(
    count > 0
      ? $t("topbar.downloads.button_aria_count", { values: { count } })
      : $t("topbar.downloads.button_aria"),
  );

  async function cancel(entry: DownloadEntry): Promise<void> {
    try {
      await cancelDownload(entry.id);
    } catch (err) {
      reportError(err, { surface: "toast" });
    }
  }

  function amount(entry: DownloadEntry): string {
    const done = formatBytes(entry.downloaded_bytes);
    if (!entry.total_bytes) return done;
    return $t("topbar.downloads.amount", {
      values: { done, total: formatBytes(entry.total_bytes) },
    });
  }

  const REASON_KEYS: Record<FailureReason, string> = {
    network: "topbar.downloads.reason.network",
    disk: "topbar.downloads.reason.disk",
    access: "topbar.downloads.reason.access",
    not_found: "topbar.downloads.reason.not_found",
    server: "topbar.downloads.reason.server",
    integrity: "topbar.downloads.reason.integrity",
    unknown: "topbar.downloads.reason.unknown",
  };

  function reasonText(entry: DownloadEntry): string {
    const { reason, status } = failureReason(entry.error);
    return $t(REASON_KEYS[reason], { values: { status: status ?? "" } });
  }
</script>

<Popover bind:open={$downloadsPanelOpen} side="bottom" align="end" class="w-80 p-0">
  {#snippet trigger(triggerProps: Record<string, unknown>)}
    <button
      {...triggerProps}
      type="button"
      class="relative flex h-7 w-7 flex-shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-surface-2 hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
      aria-label={buttonLabel}
      title={buttonLabel}
      data-testid="topbar-downloads"
    >
      <Download size={14} strokeWidth={1.5} />
      {#if count > 0}
        <span
          class="absolute -right-1 -top-1 flex h-4 min-w-4 items-center justify-center rounded-full border border-primary bg-card px-1 text-overline tabular-nums text-primary"
          data-testid="topbar-downloads-count"
        >
          {count}
        </span>
      {/if}
    </button>
  {/snippet}
  {#snippet content()}
    <div class="flex max-h-[min(28rem,70vh)] flex-col" data-testid="downloads-panel">
      <div class="border-b border-border px-3 py-2 text-body-sm font-medium text-foreground">
        {$t("topbar.downloads.title")}
      </div>
      <div class="flex flex-col gap-3 overflow-y-auto p-3">
        {#if empty}
          <p class="text-body-xs text-muted-foreground" data-testid="downloads-panel-empty">
            {$t("topbar.downloads.empty")}
          </p>
        {/if}

        {#if groups.active.length > 0}
          <section class="flex flex-col gap-2" data-testid="downloads-group-active">
            <h3 class="text-overline uppercase text-muted-foreground">
              {$t("topbar.downloads.group_active")}
            </h3>
            {#each groups.active as entry (entry.id)}
              <div class="flex items-start gap-2" data-testid="download-row-{entry.id}">
                <Download size={14} strokeWidth={1.75} class="mt-0.5 flex-shrink-0 text-primary" />
                <div class="flex min-w-0 flex-1 flex-col gap-1">
                  <span class="truncate font-mono text-caption text-foreground" title={entry.name}>
                    {entry.name}
                  </span>
                  <ProgressBar size="sm" value={downloadPercent(entry)} />
                  <span class="text-caption tabular-nums text-muted-foreground">
                    {amount(entry)} · {formatSpeed(entry.speed_bps)}
                  </span>
                </div>
                <Button
                  variant="ghost"
                  size="sm"
                  onclick={() => cancel(entry)}
                  aria-label={$t("topbar.downloads.cancel_aria", { values: { name: entry.name } })}
                  data-testid="download-cancel-{entry.id}"
                >
                  <X size={13} />
                  {$t("topbar.downloads.cancel")}
                </Button>
              </div>
            {/each}
          </section>
        {/if}

        {#if groups.finished.length > 0}
          <section class="flex flex-col gap-2" data-testid="downloads-group-finished">
            <h3 class="text-overline uppercase text-muted-foreground">
              {$t("topbar.downloads.group_finished")}
            </h3>
            {#each groups.finished as entry (entry.id)}
              <div class="flex items-start gap-2" data-testid="download-row-{entry.id}">
                {#if entry.status === "completed"}
                  <CheckCircle2 size={14} strokeWidth={1.75} class="mt-0.5 flex-shrink-0 text-success-a11y" />
                {:else}
                  <CircleSlash size={14} strokeWidth={1.75} class="mt-0.5 flex-shrink-0 text-muted-foreground" />
                {/if}
                <div class="flex min-w-0 flex-1 flex-col gap-0.5">
                  <span class="truncate font-mono text-caption text-foreground" title={entry.name}>
                    {entry.name}
                  </span>
                  <span class="text-caption text-muted-foreground" data-testid="download-status-{entry.id}">
                    {entry.status === "completed"
                      ? $t("topbar.downloads.completed")
                      : $t("topbar.downloads.cancelled")}
                  </span>
                </div>
              </div>
            {/each}
          </section>
        {/if}

        {#if groups.failed.length > 0}
          <section class="flex flex-col gap-2" data-testid="downloads-group-failed">
            <h3 class="text-overline uppercase text-muted-foreground">
              {$t("topbar.downloads.group_failed")}
            </h3>
            {#each groups.failed as entry (entry.id)}
              <div class="flex items-start gap-2" data-testid="download-row-{entry.id}">
                <AlertTriangle size={14} strokeWidth={1.75} class="mt-0.5 flex-shrink-0 text-danger-a11y" />
                <div class="flex min-w-0 flex-1 flex-col gap-0.5">
                  <span class="truncate font-mono text-caption text-foreground" title={entry.name}>
                    {entry.name}
                  </span>
                  <span
                    class="text-caption text-danger-a11y"
                    title={entry.error ?? undefined}
                    data-testid="download-reason-{entry.id}"
                  >
                    {reasonText(entry)}
                  </span>
                </div>
              </div>
            {/each}
          </section>
        {/if}
      </div>
    </div>
  {/snippet}
</Popover>
