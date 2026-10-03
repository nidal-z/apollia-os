<script lang="ts">
  /**
   * SheetActivity - what the agent did and what was changed on it.
   *
   * The executions come newest first, each one openable on the sheet (input,
   * status, output, steps). The changes are the governance history the
   * journal holds for the agent: accesses granted and revoked, settings
   * written and removed, installations and updates. The automations of the
   * agent and its execution log close the section.
   */
  import { t, locale } from "svelte-i18n";
  import { Button } from "$lib/components/ui/button";
  import { formatRelativeTime } from "$lib/utils";
  import type { TaskSummary } from "$lib/types";
  import SheetSection from "./SheetSection.svelte";
  import AgentRunRow from "./AgentRunRow.svelte";
  import AgentTriggers from "../AgentTriggers.svelte";
  import { CHANGE_LABEL, FIELD_LABEL, type ChangeView } from "./agentSheet";

  interface Props {
    runs: TaskSummary[];
    runsLoading: boolean;
    runsError: string | null;
    changes: ChangeView[];
    /** Name the automations of the agent are filed under, `null` for none. */
    triggersFor: string | null;
    /** Runtime identifier of the agent, for its execution log. */
    logsFor: string | null;
    /** The plain name of an access, by its identifier. */
    accessLabel: (accessId: string) => string;
    onOpenLogs: (agentId: string) => void;
  }

  let { runs, runsLoading, runsError, changes, triggersFor, logsFor, accessLabel, onOpenLogs }: Props =
    $props();

  let openRun = $state<string | null>(null);

  function changeSubject(change: ChangeView): string {
    if (change.kind === "override_set" || change.kind === "override_removed") {
      const key = FIELD_LABEL[change.subject ?? ""];
      return key ? $t(key) : (change.subject ?? "");
    }
    if (change.subject && change.kind.startsWith("access_")) return accessLabel(change.subject);
    return change.subject ?? "";
  }
</script>

<SheetSection
  title={$t("agent_sheet.activity_title")}
  legend={$t("agent_sheet.activity_legend")}
  testid="agent-sheet-activity"
>
  {#snippet action()}
    {#if logsFor}
      {@const id = logsFor}
      <Button variant="ghost" size="sm" onclick={() => onOpenLogs(id)} data-testid="agent-detail-logs-btn">
        {$t("agents.logs")}
      </Button>
    {/if}
  {/snippet}

  <div class="space-y-4">
    <div data-testid="agent-detail-activity">
      <div class="mb-2 text-body-xs font-semibold text-foreground">{$t("agent_sheet.runs_title")}</div>
      {#if runsLoading}
        <p class="m-0 text-body-xs text-muted-foreground">{$t("common.loading")}</p>
      {:else if runsError}
        <p class="m-0 text-body-xs text-destructive" data-testid="agent-sheet-runs-error">{runsError}</p>
      {:else if runs.length === 0}
        <p class="m-0 text-body-xs italic text-muted-foreground" data-testid="agent-sheet-runs-empty">
          {$t("agent_sheet.runs_empty")}
        </p>
      {:else}
        <ul class="m-0 list-none space-y-1.5 p-0">
          {#each runs as run (run.id)}
            <AgentRunRow
              {run}
              open={openRun === run.id}
              ontoggle={() => (openRun = openRun === run.id ? null : run.id)}
            />
          {/each}
        </ul>
      {/if}
    </div>

    <div data-testid="agent-sheet-changes">
      <div class="mb-2 text-body-xs font-semibold text-foreground">{$t("agent_sheet.changes_title")}</div>
      {#if changes.length === 0}
        <p class="m-0 text-body-xs italic text-muted-foreground" data-testid="agent-sheet-changes-empty">
          {$t("agent_sheet.changes_empty")}
        </p>
      {:else}
        <ul class="m-0 list-none space-y-1 p-0">
          {#each changes as change (change.key)}
            <li class="flex items-baseline gap-2 text-body-xs" data-testid="agent-sheet-change" data-kind={change.kind}>
              <span class="min-w-0 flex-1 text-foreground/85">
                {#if change.kind === "updated" && change.versions}
                  {$t("agent_sheet.change_updated", { values: { from: change.versions[0], to: change.versions[1] } })}
                {:else}
                  {$t(CHANGE_LABEL[change.kind], { values: { subject: changeSubject(change) } })}
                {/if}
              </span>
              <span class="shrink-0 text-caption text-muted-foreground">
                {formatRelativeTime(change.at, $locale ?? "fr")}
              </span>
            </li>
          {/each}
        </ul>
      {/if}
    </div>

    {#if triggersFor}
      <AgentTriggers agentName={triggersFor} />
    {/if}
  </div>
</SheetSection>
