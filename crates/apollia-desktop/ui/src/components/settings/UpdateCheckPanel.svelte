<script lang="ts">
  /**
   * UpdateCheckPanel - the single operator-initiated self-update surface.
   *
   * Drives the real `check_for_update` / `install_update` commands through one
   * state machine: idle -> checking -> up_to_date | available. Installing an
   * available update moves to downloading (a live progress bar fed by the
   * `update-download-progress` event, indeterminate when the backend reports no
   * total) and then to installed, just before the runtime restarts the app.
   * Failures are humanized (cause + suggested action) with the raw trace kept
   * behind a Details disclosure.
   *
   * Nothing runs in the background: a check only fires when the operator asks,
   * in line with the local-first principle.
   *
   * `variant="section"` renders inside its own titled SettingsSection (System
   * page). `variant="bare"` renders the content only, for embedding in an
   * existing section (About page).
   */
  import { onDestroy } from "svelte";
  import { t } from "svelte-i18n";
  import { Download, CheckCircle2, XCircle, RefreshCw } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { Spinner, ProgressBar } from "$lib/components/ui/progress";
  import { ErrorBanner } from "$lib/components/operator";
  import SettingsSection from "./SettingsSection.svelte";
  import { humanize, type HumanizedError } from "$lib/errors/humanize";
  import {
    checkForUpdate,
    installUpdate,
    listenUpdateDownloadProgress,
    type UpdateDownloadProgress,
  } from "$lib/ipc/updates";
  import type { UnlistenFn } from "@tauri-apps/api/event";

  interface Props {
    /** How to frame the panel: as its own section, or as bare content. */
    variant?: "section" | "bare";
  }
  let { variant = "section" }: Props = $props();

  type Phase =
    | "idle"
    | "checking"
    | "up_to_date"
    | "available"
    | "downloading"
    | "installed"
    | "error";
  type FailedAction = "check" | "install";

  let phase = $state<Phase>("idle");
  let currentVersion = $state<string | null>(null);
  let newVersion = $state<string | null>(null);
  let releaseNotes = $state<string | null>(null);
  let progress = $state<UpdateDownloadProgress | null>(null);
  let failed = $state<{ action: FailedAction; error: HumanizedError } | null>(null);

  let unlisten: UnlistenFn | null = null;

  const percent = $derived.by(() => {
    if (!progress || progress.total === null || progress.total <= 0) return undefined;
    return Math.max(0, Math.min(100, (progress.downloaded / progress.total) * 100));
  });

  function formatMib(bytes: number): string {
    return (bytes / (1024 * 1024)).toFixed(1);
  }

  const byteReadout = $derived.by(() => {
    if (!progress) return null;
    if (progress.total !== null && progress.total > 0) {
      return `${formatMib(progress.downloaded)} / ${formatMib(progress.total)} MiB`;
    }
    return `${formatMib(progress.downloaded)} MiB`;
  });

  function stopListening(): void {
    if (unlisten) {
      unlisten();
      unlisten = null;
    }
  }

  async function runCheck(): Promise<void> {
    phase = "checking";
    failed = null;
    try {
      const result = await checkForUpdate();
      currentVersion = result.current_version;
      newVersion = result.new_version ?? null;
      releaseNotes = result.release_notes ?? null;
      phase = result.available ? "available" : "up_to_date";
    } catch (err) {
      phase = "error";
      failed = { action: "check", error: humanize(err, $t) };
    }
  }

  async function runInstall(): Promise<void> {
    failed = null;
    progress = null;
    phase = "downloading";
    try {
      unlisten = await listenUpdateDownloadProgress((p) => {
        progress = p;
      });
      await installUpdate();
      phase = "installed";
    } catch (err) {
      phase = "error";
      failed = { action: "install", error: humanize(err, $t) };
    } finally {
      stopListening();
    }
  }

  function retry(): void {
    if (failed?.action === "install") void runInstall();
    else void runCheck();
  }

  onDestroy(stopListening);
</script>

{#snippet content()}
  <div
    class="flex flex-col gap-3"
    data-testid={variant === "bare" ? "update-checker" : undefined}
  >
    <div class="flex flex-wrap items-center gap-3">
      {#if phase === "idle"}
        <Button variant="outline" size="sm" onclick={runCheck} data-testid="update-check-btn">
          {#snippet icon()}<RefreshCw size={13} />{/snippet}
          {$t("settings.update.check")}
        </Button>
      {:else if phase === "checking"}
        <span class="inline-flex items-center gap-2 text-body-sm text-muted-foreground">
          <Spinner class="h-3.5 w-3.5" />
          {$t("settings.update.checking")}
        </span>
      {:else if phase === "up_to_date"}
        <span class="inline-flex items-center gap-2 text-body-sm text-foreground">
          <CheckCircle2 size={14} class="text-success" />
          {$t("settings.update.up_to_date")}
        </span>
        <Button variant="ghost" size="sm" onclick={runCheck} data-testid="update-check-btn">
          {#snippet icon()}<RefreshCw size={13} />{/snippet}
          {$t("settings.update.check")}
        </Button>
        {#if currentVersion}
          <span class="ml-auto font-mono text-caption text-muted-foreground">{currentVersion}</span>
        {/if}
      {:else if phase === "available"}
        <span class="text-label-md text-foreground" data-testid="update-available-label">
          {$t("settings.update.available", { values: { version: newVersion ?? "?" } })}
        </span>
        <Button
          variant="default"
          size="sm"
          onclick={runInstall}
          data-testid="update-install-btn"
        >
          {#snippet icon()}<Download size={13} />{/snippet}
          {$t("settings.update.install")}
        </Button>
      {:else if phase === "downloading"}
        <div class="flex w-full flex-col gap-1.5" data-testid="update-download-progress">
          <ProgressBar value={percent} label={$t("settings.update.downloading")} />
          {#if byteReadout}
            <span class="font-mono text-caption tabular-nums text-muted-foreground">
              {byteReadout}
            </span>
          {/if}
        </div>
      {:else if phase === "installed"}
        <span class="inline-flex items-center gap-2 text-body-sm text-foreground">
          <CheckCircle2 size={14} class="text-success" />
          {$t("settings.update.installed")}
        </span>
      {:else if phase === "error"}
        <span class="inline-flex items-center gap-2 text-body-sm text-danger-a11y">
          <XCircle size={14} />
          {$t("settings.update.error")}
        </span>
        <Button variant="ghost" size="sm" onclick={retry} data-testid="update-check-btn">
          {#snippet icon()}<RefreshCw size={13} />{/snippet}
          {$t("common.retry")}
        </Button>
      {/if}
    </div>

    {#if phase === "available" && releaseNotes}
      <div class="overflow-hidden rounded-lg border border-border" data-testid="update-changelog">
        <div class="border-b border-border/60 bg-muted/40 px-3.5 py-2">
          <span class="text-overline uppercase text-muted-foreground">
            {$t("settings.update.changelog_title")}
          </span>
        </div>
        <p class="whitespace-pre-wrap px-3.5 py-3 text-body-sm text-muted-foreground">
          {releaseNotes}
        </p>
      </div>
    {/if}

    {#if phase === "error" && failed}
      <ErrorBanner
        tone="danger"
        onretry={retry}
        retryLabel={$t("common.retry")}
        data-testid="update-error-banner"
      >
        <p class="font-medium">{failed.error.title}</p>
        <p class="text-caption opacity-90">{failed.error.suggested_action}</p>
        {#if failed.error.detail}
          <details class="mt-2">
            <summary
              class="cursor-pointer text-caption opacity-70 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
            >
              {$t("settings.update.show_details")}
            </summary>
            <p class="mt-1.5 break-all font-mono text-code-sm opacity-80">
              {failed.error.detail}
            </p>
          </details>
        {/if}
      </ErrorBanner>
    {/if}
  </div>
{/snippet}

{#if variant === "section"}
  <SettingsSection title={$t("settings.update.section_title")} data-testid="update-checker">
    {#snippet icon()}<Download size={15} strokeWidth={1.75} />{/snippet}
    {@render content()}
  </SettingsSection>
{:else}
  {@render content()}
{/if}
