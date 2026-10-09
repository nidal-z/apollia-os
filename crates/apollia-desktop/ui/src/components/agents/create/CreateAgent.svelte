<script lang="ts">
  /**
   * CreateAgent - an agent created from a short form, in place of the list of
   * the Agents screen.
   *
   * Four moments, one after the other:
   *
   *   - the form: what the agent is for, and what the person already knows
   *     about it;
   *   - the workshop: a workflow shipped with Apollia turns the form into a
   *     sheet, asking here what the form leaves unsaid;
   *   - the review: the sheet, checked by the runtime, opens in the form of
   *     an agent sheet, where everything stays editable;
   *   - the saved agent: its sheet, read as any other, with a way to try it.
   *
   * Nothing is written before "Save the agent". Leaving at any moment before
   * the save leaves no agent behind; a run of the workshop still going is
   * stopped when the screen is left.
   *
   * The first launch mounts the first three moments inside its own step: it
   * passes `oncreated`, which takes the saved agent in place of the last
   * moment, and `embedded`, which leaves the breadcrumb of the Agents screen
   * out.
   */
  import { onDestroy } from "svelte";
  import { locale, t } from "svelte-i18n";
  import { Bot, ChevronRight, MessageSquare } from "lucide-svelte";
  import { DetailHeader, ErrorBanner, PageHeader } from "$lib/components/operator";
  import { Banner } from "$lib/components/ui/banner";
  import { Button } from "$lib/components/ui/button";
  import { llmBackends } from "$lib/stores/llm";
  import { setAgentModel } from "$lib/ipc/agentAdjust";
  import {
    agentDraftOffer,
    checkAgentDraft,
    createAgentDefinition,
    type DraftOffer,
  } from "$lib/ipc/agentCreate";
  import type { ResolvedAgent } from "$lib/ipc/agentSheet";
  import { decidePendingDecision, listPendingDecisions } from "$lib/ipc/pendingDecisions";
  import {
    cancelWorkflowRun,
    getWorkflowRun,
    launchWorkflow,
    type WorkflowStep,
  } from "$lib/ipc/workflows";
  import AgentSheet from "../sheet/AgentSheet.svelte";
  import { modelOptions, type AdjustDraft } from "../sheet/agentAdjust";
  import { navigateTo } from "$lib/stores/navigation";
  import CreateAgentForm from "./CreateAgentForm.svelte";
  import CreateAgentProgress from "./CreateAgentProgress.svelte";
  import { modelReadiness } from "./modelReady";
  import CreateAgentReview from "./CreateAgentReview.svelte";
  import {
    UNAVAILABLE_LABEL,
    WORKSHOP_WORKFLOW,
    canPrepare,
    canSave,
    draftOfChecked,
    emptyForm,
    questionOf,
    reviewOf,
    saveDraft,
    sheetOfRun,
    stepViews,
    unavailableViews,
    workshopInputs,
    type AccessNote,
    type ProposedAgent,
    type UnavailableView,
    type WorkshopForm,
    type WorkshopQuestion,
  } from "./agentDraft";

  interface Props {
    /** Back to where the creation was asked from. */
    onback: () => void;
    /** Open a conversation with the agent `name`. */
    onStartChat?: (name: string) => void;
    onOpenLogs?: (agentId: string) => void;
    /**
     * Takes the agent once it is saved. The host then decides what follows,
     * and the sheet of the saved agent is not shown here.
     */
    oncreated?: (agent: ResolvedAgent) => void;
    /** Mounted inside another surface: no breadcrumb of the Agents screen. */
    embedded?: boolean;
  }

  let {
    onback,
    onStartChat = () => undefined,
    onOpenLogs = () => undefined,
    oncreated,
    embedded = false,
  }: Props = $props();

  /** How often the run of the workshop is read, in milliseconds. */
  const POLL_MS = 1000;

  type Phase = "form" | "drafting" | "review" | "saved";

  let phase = $state<Phase>("form");
  let wish = $state<WorkshopForm>(emptyForm());
  let offer = $state<DraftOffer | null>(null);
  let launching = $state(false);
  // Why the workshop could not start or did not hand a sheet back.
  let runError = $state<string | null>(null);
  // The declared server of the default model, when it does not answer.
  let serverDown = $state<string | null>(null);

  // The run of the workshop, while it goes on.
  let runId = $state<string | null>(null);
  let steps = $state<WorkflowStep[]>([]);
  let question = $state<WorkshopQuestion | null>(null);
  let answering = $state(false);

  // The form of the review, filled from the draft when the review opens.
  let name = $state("");
  let form = $state<AdjustDraft>({ mission: "", examples: [], access: [], sources: [], model: null });
  let listed = $state<string[]>([]);
  let checked = $state<string[]>([]);
  let notes = $state<Record<string, AccessNote>>({});
  let missing = $state<UnavailableView[]>([]);
  // The draft the form was last filled from, `null` before the first review.
  let reviewedKey: string | null = null;
  // A sheet was opened already: the form offers the way back to it.
  let reviewed = $state(false);

  let saving = $state(false);
  let saveError = $state<string | null>(null);
  let created = $state<ResolvedAgent | null>(null);
  let modelError = $state<string | null>(null);

  let pollTimer: ReturnType<typeof setInterval> | undefined;
  let following = false;

  function message(err: unknown): string {
    return err instanceof Error ? err.message : String(err);
  }

  function stopPolling(): void {
    if (pollTimer !== undefined) {
      clearInterval(pollTimer);
      pollTimer = undefined;
    }
  }

  // What the machine holds, for the boxes of the form. A machine that cannot
  // say leaves the form with its text fields.
  void agentDraftOffer()
    .then((read) => (offer = read))
    .catch(() => undefined);

  /** Back to the form, saying why the workshop handed no sheet. */
  function fail(reason: string): void {
    stopPolling();
    runId = null;
    question = null;
    runError = reason;
    phase = "form";
  }

  /** Read the run: its steps, the question it waits on, then its sheet. */
  async function follow(): Promise<void> {
    const run = runId;
    if (!run || following) return;
    following = true;
    try {
      const detail = await getWorkflowRun(run);
      if (runId !== run) return;
      steps = detail.steps;
      if (detail.run.status === "running") {
        question = questionOf(await listPendingDecisions().catch(() => []), run);
        return;
      }
      stopPolling();
      question = null;
      if (detail.run.status !== "completed") {
        fail(detail.run.error ?? $t("agent_create.run_stopped"));
        return;
      }
      const sheet = sheetOfRun(detail.run.output);
      if (!sheet) {
        fail($t("agent_create.run_no_sheet"));
        return;
      }
      let draft: ProposedAgent | null;
      try {
        draft = draftOfChecked(await checkAgentDraft(sheet));
      } catch (err) {
        fail(message(err));
        return;
      }
      if (!draft) {
        fail($t("agent_create.run_no_sheet"));
        return;
      }
      runId = null;
      openReview(draft);
    } catch {
      // Read again on the next tick.
    } finally {
      following = false;
    }
  }

  /** Launch the workshop on the form. */
  async function prepare(): Promise<void> {
    if (launching || !canPrepare(wish)) return;
    launching = true;
    runError = null;
    serverDown = null;
    try {
      const model = await modelReadiness();
      if (!model.ready) {
        serverDown = model.server;
        return;
      }
      // Read again at launch: a source added since the form opened counts.
      offer = await agentDraftOffer().catch(() => offer);
      const run = await launchWorkflow(WORKSHOP_WORKFLOW, workshopInputs(wish, offer, $locale));
      runId = run.runId;
      steps = [];
      question = null;
      phase = "drafting";
      pollTimer = setInterval(() => void follow(), POLL_MS);
    } catch (err) {
      runError = message(err);
    } finally {
      launching = false;
    }
  }

  /** Answer the question the run waits on; `null` passes it. */
  async function answer(text: string | null): Promise<void> {
    const asked = question;
    if (!asked || answering) return;
    answering = true;
    try {
      await decidePendingDecision(
        text === null ? { id: asked.id, answer: "refuse" } : { id: asked.id, answer: "answer", value: text },
      );
      question = null;
    } catch (err) {
      runError = message(err);
    } finally {
      answering = false;
    }
  }

  /** Stop the run and go back to the form, which keeps what was typed. */
  function backToForm(): void {
    stopPolling();
    if (runId) void cancelWorkflowRun(runId).catch(() => undefined);
    runId = null;
    question = null;
    phase = "form";
  }

  onDestroy(() => {
    stopPolling();
    if (runId) void cancelWorkflowRun(runId).catch(() => undefined);
  });

  /** Open the form on `proposal`, or empty for a sheet written by hand. */
  function openReview(proposal: ProposedAgent | null): void {
    // A draft already opened keeps the edits made in the form; a newer one
    // replaces them.
    const key = JSON.stringify(proposal);
    if (key !== reviewedKey) {
      const review = reviewOf(proposal, modelOptions($llmBackends));
      name = review.name;
      form = review.form;
      listed = review.listed;
      checked = review.checked;
      notes = review.notes;
      missing = unavailableViews(review.unavailable);
      reviewedKey = key;
    }
    saveError = null;
    reviewed = true;
    phase = "review";
  }

  async function save(): Promise<void> {
    if (saving) return;
    saving = true;
    saveError = null;
    try {
      const outcome = await saveDraft(name, $state.snapshot(form) as AdjustDraft, {
        create: createAgentDefinition,
        setModel: (agent, model) => setAgentModel(agent, model.model_source, model.model_id, model.preset_id),
      });
      if (!outcome) return;
      if (oncreated) {
        oncreated(outcome.agent);
        return;
      }
      created = outcome.agent;
      modelError = outcome.modelError;
      phase = "saved";
    } catch (err) {
      saveError = message(err);
    } finally {
      saving = false;
    }
  }

  const savable = $derived(canSave(name, form));
  const preparable = $derived(canPrepare(wish));
  const progress = $derived(stepViews(steps));
</script>

<div class="flex h-full min-h-0 flex-col" data-testid="agent-create" data-phase={phase}>
  {#if !embedded}
    <nav
      class="flex items-center gap-1 px-8 pt-5 text-caption text-muted-foreground"
      aria-label={$t("a11y.breadcrumb")}
    >
      <button
        type="button"
        class="rounded px-1 hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        onclick={onback}
        data-testid="agent-create-back"
      >
        {$t("agents.title")}
      </button>
      <ChevronRight size={12} />
      <span class="text-foreground">{$t("agent_create.breadcrumb")}</span>
    </nav>
  {/if}

  {#if phase === "saved" && created}
    <DetailHeader title={created.name} titleTestid="agent-create-saved-title" meta={$t("agent_create.saved_meta")}>
      {#snippet leading()}
        <span
          class="inline-flex h-10 w-10 shrink-0 items-center justify-center rounded-lg bg-muted text-foreground"
        >
          <Bot size={18} />
        </span>
      {/snippet}
    </DetailHeader>
    <div class="min-h-0 flex-1 space-y-5 overflow-y-auto px-8 pb-8 pt-5">
      <Banner variant="success" surface="card" data-testid="agent-create-saved">
        {$t("agent_create.saved_banner", { values: { name: created.name } })}
        {#snippet trailing()}
          <Button
            variant="primary-solid"
            size="sm"
            onclick={() => created && onStartChat(created.id)}
            data-testid="agent-create-try"
          >
            {#snippet icon()}<MessageSquare size={13} />{/snippet}
            {$t("agent_create.try_in_chat")}
          </Button>
        {/snippet}
      </Banner>
      {#if modelError}
        <Banner variant="warning" surface="card" data-testid="agent-create-model-error">
          {$t("agent_create.model_not_assigned", { values: { reason: modelError } })}
        </Banner>
      {/if}
      <AgentSheet definitionId={created.id} agent={null} memoryNamespace={null} {onOpenLogs} />
    </div>
  {:else if phase === "review"}
    <PageHeader
      kicker={$t("agent_create.kicker")}
      title={$t("agent_create.review_title")}
      subtitle={$t("agent_create.review_subtitle")}
    >
      {#snippet actions()}
        <Button
          variant="outline"
          size="sm"
          onclick={() => (phase = "form")}
          disabled={saving}
          data-testid="agent-create-review-back"
        >
          {$t("agent_create.review_back")}
        </Button>
        <Button
          variant="primary-solid"
          size="sm"
          onclick={() => void save()}
          disabled={!savable || saving}
          data-testid="agent-create-save"
        >
          {saving ? $t("agent_create.saving") : $t("agent_create.save")}
        </Button>
      {/snippet}
    </PageHeader>
    <div class="min-h-0 flex-1 space-y-5 overflow-y-auto px-8 pb-8 pt-5">
      {#if saveError}
        <ErrorBanner data-testid="agent-create-save-error">
          <p class="font-medium">{$t("agent_create.save_failed")}</p>
          <p class="text-caption opacity-90">{saveError}</p>
        </ErrorBanner>
      {/if}
      {#if missing.length > 0}
        <Banner variant="warning" surface="card" data-testid="agent-create-unavailable">
          <p class="m-0 font-medium">{$t("agent_create.unavailable_title")}</p>
          <ul class="m-0 mt-1 list-none space-y-0.5 p-0">
            {#each missing as entry (entry.kind + entry.name)}
              <li class="text-caption">
                {$t(UNAVAILABLE_LABEL[entry.kind], { values: { name: entry.name } })}
              </li>
            {/each}
          </ul>
          <p class="m-0 mt-1 text-caption opacity-90">{$t("agent_create.unavailable_hint")}</p>
        </Banner>
      {/if}
      <CreateAgentReview bind:name {form} {listed} {checked} {notes} />
    </div>
  {:else if phase === "drafting"}
    <PageHeader
      kicker={$t("agent_create.kicker")}
      title={$t("agent_create.drafting_title")}
      subtitle={$t("agent_create.drafting_subtitle")}
    >
      {#snippet actions()}
        <Button variant="outline" size="sm" onclick={backToForm} data-testid="agent-create-drafting-back">
          {$t("agent_create.review_back")}
        </Button>
      {/snippet}
    </PageHeader>
    <div class="min-h-0 flex-1 space-y-5 overflow-y-auto px-8 pb-8 pt-5">
      {#if runError}
        <ErrorBanner data-testid="agent-create-answer-error">
          <p class="text-caption opacity-90">{runError}</p>
        </ErrorBanner>
      {/if}
      <CreateAgentProgress steps={progress} {question} {answering} onanswer={(text) => void answer(text)} />
    </div>
  {:else}
    <PageHeader
      kicker={$t("agent_create.kicker")}
      title={$t("agent_create.title")}
      subtitle={$t("agent_create.subtitle")}
    >
      {#snippet actions()}
        <Button variant="outline" size="sm" onclick={onback} data-testid="agent-create-cancel">
          {$t("common.cancel")}
        </Button>
        {#if reviewed}
          <Button variant="outline" size="sm" onclick={() => (phase = "review")} data-testid="agent-create-review-resume">
            {$t("agent_create.review_resume")}
          </Button>
        {/if}
        <Button variant="outline" size="sm" onclick={() => openReview(null)} data-testid="agent-create-by-hand">
          {$t("agent_create.write_by_hand")}
        </Button>
        <Button
          variant="primary-solid"
          size="sm"
          onclick={() => void prepare()}
          disabled={!preparable || launching}
          data-testid="agent-create-prepare"
        >
          {launching ? $t("agent_create.preparing") : $t("agent_create.prepare")}
        </Button>
      {/snippet}
    </PageHeader>
    <div class="min-h-0 flex-1 space-y-5 overflow-y-auto px-8 pb-8 pt-5">
      {#if serverDown !== null}
        <div class="max-w-3xl space-y-3" data-testid="agent-create-server-down">
          <ErrorBanner>
            <p class="font-medium">{$t("agent_create.server_down_title")}</p>
            <p class="text-caption opacity-90">
              {$t("agent_create.server_down_body", { values: { server: serverDown } })}
            </p>
          </ErrorBanner>
          <div class="flex flex-wrap gap-2">
            <Button variant="outline" size="sm" onclick={() => void prepare()} data-testid="agent-create-server-retry">
              {$t("common.retry")}
            </Button>
            {#if !embedded}
              <Button variant="outline" size="sm" onclick={() => navigateTo("llm")} data-testid="agent-create-open-ai">
                {$t("agent_create.open_ai")}
              </Button>
            {/if}
          </div>
        </div>
      {:else if runError !== null}
        <div class="max-w-3xl" data-testid="agent-create-run-error">
          <ErrorBanner>
            <p class="font-medium">{$t("agent_create.run_failed")}</p>
            <p class="text-caption opacity-90">{runError}</p>
          </ErrorBanner>
        </div>
      {/if}
      <CreateAgentForm form={wish} {offer} />
    </div>
  {/if}
</div>
