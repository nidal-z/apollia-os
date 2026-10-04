<script lang="ts">
  /**
   * ProjectMigrationNotice - the notice shown once after the former projects
   * became sources: what each part of a project became, what could not be
   * carried over, and where to find the rest.
   *
   * It reads the report of the runtime at mount and shows nothing when no
   * migration has run, or when this outcome was already shown.
   */
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import { AlertTriangle, BookOpen, Bot, MessageSquare, Plug } from "lucide-svelte";
  import Dialog from "$lib/components/ui/dialog/Dialog.svelte";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { getProjectMigrationReport } from "$lib/ipc/projectMigration";
  import { navigateTo, type Route } from "$lib/stores/navigation";
  import {
    noticeModel,
    readSeen,
    recordSeen,
    shouldShow,
    type MigrationIssue,
    type MigrationReport,
  } from "$lib/projectMigration/notice";

  let report = $state<MigrationReport | null>(null);
  let open = $state(false);
  const model = $derived(report ? noticeModel(report) : null);

  function storage(): Storage | undefined {
    try {
      return typeof localStorage === "undefined" ? undefined : localStorage;
    } catch {
      return undefined;
    }
  }

  onMount(() => {
    void getProjectMigrationReport()
      .then((found) => {
        if (!shouldShow(found, readSeen(storage()))) return;
        report = found;
        open = true;
      })
      .catch(() => {
        /* no report reachable: nothing to tell */
      });
  });

  function close(): void {
    if (report) recordSeen(storage(), report);
    open = false;
  }

  function go(route: Route): void {
    close();
    navigateTo(route);
  }

  /** The base name of a path, or the subject as it is. */
  function shortName(subject: string): string {
    const parts = subject.split(/[\\/]/);
    return parts[parts.length - 1] || subject;
  }

  function gapLine(issue: MigrationIssue): string {
    const values = {
      subject: shortName(issue.subject),
      project: issue.projectName ?? "",
      reason: $t(`project_migration.reason.${issue.reason}`),
    };
    return issue.projectName
      ? $t("project_migration.gap_line", { values })
      : $t("project_migration.gap_line_no_project", { values });
  }
</script>

{#if model}
  <Dialog
    {open}
    onclose={close}
    size="lg"
    title={$t(
      model.kind === "failed" ? "project_migration.title_failed" : "project_migration.title_done",
    )}
    hideClose
    data-testid="project-migration-notice"
  >
    <div class="flex flex-col gap-4" data-kind={model.kind}>
      <p class="text-caption text-muted-foreground" data-testid="project-migration-subtitle">
        {$t(
          model.kind === "failed"
            ? "project_migration.subtitle_failed"
            : "project_migration.subtitle_done",
        )}
      </p>

      {#if model.kind !== "failed"}
        <section class="flex flex-col gap-2" data-testid="project-migration-became">
          <h3 class="text-caption text-muted-foreground">
            {$t("project_migration.became_title", { values: { count: model.projects } })}
          </h3>
          <ul class="flex flex-col divide-y divide-border/40 rounded-lg border border-border/60">
            {#if model.documents > 0}
              <li class="flex items-center gap-3 px-4 py-2.5" data-testid="project-migration-row-documents">
                <BookOpen size={14} class="shrink-0 text-muted-foreground" />
                <div class="min-w-0 flex-1">
                  <div class="text-body-sm text-foreground">{$t("project_migration.row_documents_title")}</div>
                  <div class="text-micro text-muted-foreground">
                    {$t("project_migration.row_documents_desc", { values: { count: model.documents } })}
                  </div>
                </div>
                <Badge variant="success">{$t("project_migration.badge_done")}</Badge>
              </li>
            {/if}
            {#if model.instructions > 0}
              <li class="flex items-center gap-3 px-4 py-2.5" data-testid="project-migration-row-instructions">
                <BookOpen size={14} class="shrink-0 text-muted-foreground" />
                <div class="min-w-0 flex-1">
                  <div class="text-body-sm text-foreground">{$t("project_migration.row_instructions_title")}</div>
                  <div class="text-micro text-muted-foreground">{$t("project_migration.row_instructions_desc")}</div>
                </div>
                <Badge variant="success">{$t("project_migration.badge_done")}</Badge>
              </li>
            {/if}
            {#if model.folders > 0}
              <li class="flex items-center gap-3 px-4 py-2.5" data-testid="project-migration-row-folders">
                <Plug size={14} class="shrink-0 text-primary" />
                <div class="min-w-0 flex-1">
                  <div class="text-body-sm text-foreground">{$t("project_migration.row_folders_title")}</div>
                  <div class="text-micro text-muted-foreground">
                    {$t("project_migration.row_folders_desc", { values: { count: model.folders } })}
                  </div>
                </div>
                <Badge variant="warning">{$t("project_migration.badge_to_grant")}</Badge>
              </li>
            {/if}
            <li class="flex items-center gap-3 px-4 py-2.5" data-testid="project-migration-row-conversations">
              <MessageSquare size={14} class="shrink-0 text-primary" />
              <div class="min-w-0 flex-1">
                <div class="text-body-sm text-foreground">{$t("project_migration.row_conversations_title")}</div>
                <div class="text-micro text-muted-foreground">{$t("project_migration.row_conversations_desc")}</div>
              </div>
              <Badge variant="success">{$t("project_migration.badge_done")}</Badge>
            </li>
          </ul>
        </section>
      {/if}

      {#if model.kind === "failed"}
        <div class="flex gap-3 rounded-lg bg-destructive/10 px-4 py-3" data-testid="project-migration-failed">
          <AlertTriangle size={14} class="mt-0.5 shrink-0 text-destructive" />
          <div class="min-w-0">
            <div class="text-body-sm text-foreground">{$t("project_migration.failed_title")}</div>
            <p class="text-micro text-foreground/80">{$t("project_migration.failed_body")}</p>
          </div>
        </div>
      {:else if model.notCarried.length > 0}
        <div class="flex gap-3 rounded-lg bg-warning/10 px-4 py-3" data-testid="project-migration-gaps">
          <AlertTriangle size={14} class="mt-0.5 shrink-0 text-warning-a11y" />
          <div class="min-w-0">
            <div class="text-body-sm text-foreground">
              {$t("project_migration.gaps_title", { values: { count: model.notCarried.length } })}
            </div>
            <ul class="mt-1 flex flex-col gap-0.5">
              {#each model.notCarried as issue, index (index)}
                <li class="text-micro text-foreground/80 [overflow-wrap:anywhere]" data-testid="project-migration-gap">
                  {gapLine(issue)}
                </li>
              {/each}
            </ul>
            <p class="mt-1 text-micro text-foreground/80">
              {$t(
                model.kind === "partial"
                  ? "project_migration.gaps_partial_hint"
                  : "project_migration.gaps_complete_hint",
              )}
            </p>
          </div>
        </div>
      {/if}

      {#if model.kind !== "failed"}
        <section class="flex flex-col gap-2" data-testid="project-migration-where">
          <h3 class="text-caption text-muted-foreground">{$t("project_migration.where_title")}</h3>
          <ul class="flex flex-col divide-y divide-border/40 rounded-lg border border-border/60">
            <li class="flex items-center gap-3 px-4 py-2.5">
              <Bot size={14} class="shrink-0 text-primary" />
              <div class="min-w-0 flex-1">
                <div class="text-body-sm text-foreground">{$t("project_migration.where_sources_title")}</div>
                <div class="text-micro text-muted-foreground">{$t("project_migration.where_sources_desc")}</div>
              </div>
              <Button variant="link" size="sm" onclick={() => go("agents")} data-testid="project-migration-open-sources">
                {$t("project_migration.open")}
              </Button>
            </li>
            {#if model.folders > 0}
              <li class="flex items-center gap-3 px-4 py-2.5">
                <Plug size={14} class="shrink-0 text-primary" />
                <div class="min-w-0 flex-1">
                  <div class="text-body-sm text-foreground">{$t("project_migration.where_connections_title")}</div>
                  <div class="text-micro text-muted-foreground">
                    {$t("project_migration.where_connections_desc", { values: { count: model.folders } })}
                  </div>
                </div>
                <Button variant="link" size="sm" onclick={() => go("integrations")} data-testid="project-migration-open-connections">
                  {$t("project_migration.open")}
                </Button>
              </li>
            {/if}
            <li class="flex items-center gap-3 px-4 py-2.5">
              <MessageSquare size={14} class="shrink-0 text-primary" />
              <div class="min-w-0 flex-1">
                <div class="text-body-sm text-foreground">{$t("project_migration.where_chat_title")}</div>
                <div class="text-micro text-muted-foreground">{$t("project_migration.where_chat_desc")}</div>
              </div>
              <Button variant="link" size="sm" onclick={() => go("chat")} data-testid="project-migration-open-chat">
                {$t("project_migration.open")}
              </Button>
            </li>
          </ul>
        </section>
      {/if}

      <div class="flex items-center gap-3">
        <p class="flex-1 text-caption text-muted-foreground">
          {$t(model.kind === "complete" ? "project_migration.note_once" : "project_migration.note_retry")}
        </p>
        <Button variant="primary-solid" onclick={close} data-testid="project-migration-understood">
          {$t("project_migration.understood")}
        </Button>
      </div>
    </div>
  </Dialog>
{/if}
