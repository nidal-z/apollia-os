<script lang="ts">
  /**
   * WorkflowSheet - the sheet of one workflow, read like the sheet of an
   * agent: what it does, its steps, what it may do, and its history of runs.
   *
   * "Launch" opens the form generated from the inputs the workflow declares.
   * A run going is shown above the steps, which then say where each one
   * stands, and it can be stopped. A run of the history opens the same way;
   * an ended run offers to launch again with its inputs.
   */
  import { t, locale } from "svelte-i18n";
  import { ChevronRight, Package, Play, Plug, Workflow } from "lucide-svelte";
  import { ErrorBanner } from "$lib/components/operator";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { navigateTo } from "$lib/stores/navigation";
  import { formatRelativeTime } from "$lib/utils";
  import SheetSection from "../agents/sheet/SheetSection.svelte";
  import WorkflowLaunchDialog from "./WorkflowLaunchDialog.svelte";
  import WorkflowRunCard from "./WorkflowRunCard.svelte";
  import WorkflowRunRow from "./WorkflowRunRow.svelte";
  import WorkflowStepRow from "./WorkflowStepRow.svelte";
  import { createWorkflowSheet } from "./useWorkflowSheet.svelte";
  import {
    familyLabelKey,
    isRunning,
    lastRun,
    runLabelKey,
    runningRuns,
    shownRuns,
    splitWorkflowId,
  } from "./workflowRuns";

  interface Props {
    /** `<package>/<workflow>`. */
    workflowId: string;
    /** The name the list showed, until the workflow is read. */
    name: string;
    /** Opens the launch form as soon as the workflow is read. */
    launchOnOpen?: boolean;
    onback: () => void;
  }

  let { workflowId, name, launchOnOpen = false, onback }: Props = $props();

  /** Runs the history shows before "See all". */
  const HISTORY_PREVIEW = 4;

  const sheet = createWorkflowSheet(() => workflowId);
  const ids = $derived(splitWorkflowId(workflowId));

  let launchOpen = $state(false);
  /** The inputs the form opens with, when launching a past run again. */
  let previous = $state<Record<string, unknown> | undefined>(undefined);
  let allRuns = $state(false);
  let asked = $state(false);

  const workflow = $derived(sheet.workflow);
  const open = $derived(sheet.open);
  const last = $derived(lastRun(sheet.runs));
  const going = $derived(runningRuns(sheet.runs).length);
  const ended = $derived(open !== null && !isRunning(open.run));
  const history = $derived(shownRuns(sheet.runs, allRuns, HISTORY_PREVIEW));
  const stepsByName = $derived(new Map((open?.steps ?? []).map((step) => [step.name, step])));

  // The list asked for a launch: the form opens once, when the fields are known.
  $effect(() => {
    if (!launchOnOpen || asked || workflow === null) return;
    asked = true;
    openLaunch(undefined);
  });

  function openLaunch(inputs: Record<string, unknown> | undefined): void {
    previous = inputs;
    launchOpen = true;
  }
</script>

<div class="flex flex-col gap-6 px-8 py-8" data-testid="workflow-sheet" data-workflow={workflowId}>
  <nav class="flex items-center gap-1 text-caption text-muted-foreground" aria-label={$t("a11y.breadcrumb")}>
    <button
      type="button"
      class="rounded px-1 hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
      onclick={onback}
      data-testid="workflow-sheet-back"
    >
      {$t("agents.title")}
    </button>
    <ChevronRight size={12} />
    <span class="text-foreground">{$t("workflows.kicker")}</span>
  </nav>

  <header class="flex flex-wrap items-start gap-4">
    <span class="flex h-11 w-11 shrink-0 items-center justify-center rounded-lg bg-primary/10 text-primary">
      <Workflow size={22} />
    </span>
    <div class="min-w-0 flex-1">
      <h1 class="m-0 text-heading-lg font-semibold text-foreground" data-testid="workflow-sheet-title">{name}</h1>
      <p class="m-0 text-caption text-muted-foreground" data-testid="workflow-sheet-meta">
        {$t("workflows.meta_package", { values: { package: ids.package, version: workflow?.version ?? "" } })}
        {#if workflow}
          · {$t("workflows.steps_count", { values: { count: workflow.steps.length } })}
        {/if}
        {#if going > 0}
          · {$t("workflows.runs_going", { values: { count: going } })}
        {:else if last}
          · {$t("workflows.meta_last_run", {
            values: {
              when: formatRelativeTime(last.startedAt, $locale ?? "fr"),
              outcome: $t(runLabelKey(last.status)).toLowerCase(),
            },
          })}
        {/if}
      </p>
    </div>
    <div class="flex items-center gap-2">
      <Button variant="outline" size="sm" onclick={() => navigateTo("library")} data-testid="workflow-sheet-package">
        {#snippet icon()}<Package size={12} />{/snippet}
        {$t("workflows.see_package")}
      </Button>
      <Button
        variant="primary-solid"
        size="sm"
        disabled={workflow === null}
        onclick={() => openLaunch(ended ? open?.run.inputs : undefined)}
        data-testid="workflow-launch-button"
      >
        {#snippet icon()}<Play size={12} />{/snippet}
        {ended ? $t("workflows.launch_again") : $t("workflows.launch")}
      </Button>
    </div>
  </header>

  {#if sheet.loadError}
    <ErrorBanner data-testid="workflow-sheet-error">
      <p class="m-0 font-medium">{sheet.loadError}</p>
    </ErrorBanner>
  {:else if sheet.loading || workflow === null}
    <p class="m-0 text-body-xs text-muted-foreground" data-testid="workflow-sheet-loading">{$t("common.loading")}</p>
  {:else}
    {#if open}
      <WorkflowRunCard
        detail={open}
        now={sheet.now}
        stopping={sheet.stopping}
        onstop={() => void sheet.stop()}
        onrelaunch={() => openLaunch(open.run.inputs)}
        onclose={sheet.closeRun}
      />
    {/if}

    <div class="grid grid-cols-1 gap-6 lg:grid-cols-2">
      <div class="space-y-6">
        <SheetSection
          title={$t("workflows.what_title")}
          legend={$t("workflows.what_legend")}
          testid="workflow-sheet-what"
        >
          <p class="m-0 whitespace-pre-wrap text-body-xs text-foreground/85">
            {workflow.description || $t("workflows.what_none")}
          </p>
        </SheetSection>

        <SheetSection
          title={open ? $t("workflows.run_steps_title") : $t("workflows.steps_title")}
          legend={open ? $t("workflows.run_steps_legend") : $t("workflows.steps_legend")}
          testid="workflow-sheet-steps"
        >
          {#if workflow.steps.length === 0}
            <p class="m-0 text-body-xs italic text-muted-foreground">{$t("workflows.steps_none")}</p>
          {:else}
            <ol class="m-0 list-none space-y-2 p-0">
              {#each workflow.steps as step, index (step)}
                <WorkflowStepRow position={index + 1} name={step} step={stepsByName.get(step)} now={sheet.now} />
              {/each}
            </ol>
          {/if}
        </SheetSection>
      </div>

      <div class="space-y-6">
        <SheetSection
          title={$t("workflows.access_title")}
          legend={$t("workflows.access_legend")}
          testid="workflow-sheet-access"
        >
          <ul class="m-0 list-none space-y-1 p-0">
            {#if sheet.access}
              <li class="flex items-center gap-2 py-1" data-testid="workflow-access-family" data-family={sheet.access.family}>
                <span class="min-w-0 flex-1 text-body-xs text-foreground/85">
                  {$t(familyLabelKey(sheet.access.family))}
                </span>
                {#if sheet.access.sensitive}
                  <Badge size="sm" variant="warning">{$t("agent_sheet.access_sensitive")}</Badge>
                {/if}
              </li>
            {:else}
              <li class="py-1 text-body-xs italic text-muted-foreground" data-testid="workflow-access-unknown">
                {$t("workflows.access_unknown")}
              </li>
            {/if}
            {#each sheet.connections as connection (connection.type)}
              <li class="flex items-start gap-2 border-t border-border/40 py-1.5" data-testid="workflow-access-connection">
                <Plug size={13} class="mt-0.5 shrink-0 text-muted-foreground" />
                <div class="min-w-0 flex-1">
                  <div class="text-body-xs text-foreground/85">
                    {$t("workflows.access_connection", { values: { type: connection.type } })}
                  </div>
                  {#if connection.reason}
                    <div class="text-caption text-muted-foreground">{connection.reason}</div>
                  {/if}
                </div>
              </li>
            {/each}
          </ul>
        </SheetSection>

        <SheetSection
          title={$t("workflows.history_title")}
          legend={$t("workflows.history_legend")}
          testid="workflow-sheet-history"
        >
          {#snippet action()}
            {#if sheet.runs.length > HISTORY_PREVIEW && !allRuns}
              <Button variant="ghost" size="sm" onclick={() => (allRuns = true)} data-testid="workflow-history-all">
                {$t("workflows.history_all")}
              </Button>
            {/if}
          {/snippet}
          {#if sheet.runs.length === 0}
            <p class="m-0 text-body-xs italic text-muted-foreground" data-testid="workflow-history-empty">
              {$t("workflows.history_empty")}
            </p>
          {:else}
            <ul class="m-0 list-none space-y-1.5 p-0">
              {#each history as run (run.runId)}
                <WorkflowRunRow
                  {run}
                  selected={open?.run.runId === run.runId}
                  now={sheet.now}
                  onopen={() => void sheet.openRun(run.runId)}
                />
              {/each}
            </ul>
          {/if}
        </SheetSection>
      </div>
    </div>

    <WorkflowLaunchDialog
      open={launchOpen}
      {name}
      fields={workflow.inputs}
      {previous}
      access={sheet.access}
      onlaunch={(inputs) => sheet.launch(inputs)}
      onclose={() => (launchOpen = false)}
    />
  {/if}
</div>
