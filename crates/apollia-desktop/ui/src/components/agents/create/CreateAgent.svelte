<script lang="ts">
  /**
   * CreateAgent - an agent created by describing it, in place of the list of
   * the Agents screen.
   *
   * Three moments, one after the other:
   *
   *   - the conversation: a short exchange with the Apollia agent, which
   *     drafts the sheet as it goes; the draft fills up beside it;
   *   - the review: the draft opens in the form of an agent sheet, where
   *     everything stays editable;
   *   - the saved agent: its sheet, read as any other, with a way to try it.
   *
   * Nothing is written before "Save the agent": the conversation proposes,
   * the draft is read in the conversation, and the form lives in this
   * component. Leaving at any moment before the save leaves no agent behind;
   * the conversation itself is removed when the screen is left.
   *
   * The first launch mounts the first two moments inside its own step: it
   * passes `oncreated`, which takes the saved agent in place of the third
   * moment, and `embedded`, which leaves the breadcrumb of the Agents screen
   * out.
   */
  import { onDestroy } from "svelte";
  import { t } from "svelte-i18n";
  import { Bot, ChevronRight, MessageSquare } from "lucide-svelte";
  import { DetailHeader, ErrorBanner, PageHeader } from "$lib/components/operator";
  import { LoadingSpinner } from "$lib/components/feedback";
  import { Banner } from "$lib/components/ui/banner";
  import { Button } from "$lib/components/ui/button";
  import { llmBackends } from "$lib/stores/llm";
  import { createChatSession } from "$lib/ipc/agents";
  import { deleteChatSession, getChatSession, sendChatMessage } from "$lib/ipc/chat";
  import { setAgentModel } from "$lib/ipc/agentAdjust";
  import { createAgentDefinition } from "$lib/ipc/agentCreate";
  import type { ResolvedAgent } from "$lib/ipc/agentSheet";
  import ChatConversation from "../../chat/ChatConversation.svelte";
  import AgentSheet from "../sheet/AgentSheet.svelte";
  import { modelOptions, type AdjustDraft } from "../sheet/agentAdjust";
  import { navigateTo } from "$lib/stores/navigation";
  import CreateAgentDraft from "./CreateAgentDraft.svelte";
  import { modelReadiness } from "./modelReady";
  import CreateAgentReview from "./CreateAgentReview.svelte";
  import {
    DRAFT_SESSION_REQUEST,
    canSave,
    extractDraft,
    reviewOf,
    saveDraft,
    type AccessNote,
    type ProposedAgent,
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

  /** How often the conversation is read for a new draft, in milliseconds. */
  const POLL_MS = 1500;

  type Phase = "conversation" | "review" | "saved";

  let phase = $state<Phase>("conversation");
  let sessionId = $state<string | null>(null);
  let starting = $state(true);
  let startError = $state<string | null>(null);
  // The declared server of the default model, when it does not answer.
  let serverDown = $state<string | null>(null);
  let draft = $state<ProposedAgent | null>(null);
  let answering = $state(false);

  // The form of the review, filled from the draft when the review opens.
  let name = $state("");
  let form = $state<AdjustDraft>({ mission: "", examples: [], access: [], sources: [], model: null });
  let listed = $state<string[]>([]);
  let checked = $state<string[]>([]);
  let notes = $state<Record<string, AccessNote>>({});
  // The draft the form was last filled from, `null` before the first review.
  let reviewedKey: string | null = null;

  let saving = $state(false);
  let saveError = $state<string | null>(null);
  let created = $state<ResolvedAgent | null>(null);
  let modelError = $state<string | null>(null);

  let pollTimer: ReturnType<typeof setInterval> | undefined;

  function message(err: unknown): string {
    return err instanceof Error ? err.message : String(err);
  }

  function stopPolling(): void {
    if (pollTimer !== undefined) {
      clearInterval(pollTimer);
      pollTimer = undefined;
    }
  }

  /** Read the latest draft the conversation holds, if any. */
  async function readDraft(): Promise<void> {
    if (!sessionId) return;
    try {
      const detail = await getChatSession(sessionId);
      answering = detail.status === "processing";
      const latest = extractDraft(detail.messages);
      if (latest && JSON.stringify(latest) !== JSON.stringify(draft)) draft = latest;
    } catch {
      // Read again on the next tick.
    }
  }

  async function start(): Promise<void> {
    starting = true;
    startError = null;
    serverDown = null;
    const model = await modelReadiness();
    if (!model.ready) {
      serverDown = model.server;
      starting = false;
      return;
    }
    try {
      const session = await createChatSession(DRAFT_SESSION_REQUEST);
      sessionId = session.id;
      // The first message is translated, and not for cosmetics: it is what the
      // model reads to choose the language it answers in.
      await sendChatMessage(session.id, $t("agent_create.kick_message")).catch(() => undefined);
      pollTimer = setInterval(() => void readDraft(), POLL_MS);
    } catch (err) {
      startError = message(err);
    } finally {
      starting = false;
    }
  }

  void start();

  onDestroy(() => {
    stopPolling();
    // The conversation only served the draft: it is not kept among the chats.
    if (sessionId) void deleteChatSession(sessionId).catch(() => undefined);
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
      reviewedKey = key;
    }
    saveError = null;
    phase = "review";
  }

  async function review(): Promise<void> {
    await readDraft();
    openReview(draft);
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
      stopPolling();
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
        {#if sessionId}
          <Button
            variant="outline"
            size="sm"
            onclick={() => (phase = "conversation")}
            disabled={saving}
            data-testid="agent-create-review-back"
          >
            {$t("agent_create.review_back")}
          </Button>
        {:else}
          <Button variant="outline" size="sm" onclick={onback} disabled={saving} data-testid="agent-create-cancel">
            {$t("common.cancel")}
          </Button>
        {/if}
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
      <CreateAgentReview bind:name {form} {listed} {checked} {notes} />
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
        <Button
          variant="primary-solid"
          size="sm"
          onclick={() => void review()}
          disabled={draft === null}
          data-testid="agent-create-review-open"
        >
          {$t("agent_create.review_open")}
        </Button>
      {/snippet}
    </PageHeader>
    <div class="grid min-h-0 flex-1 grid-cols-1 gap-6 px-8 pb-6 pt-5 lg:grid-cols-[minmax(0,3fr)_minmax(0,2fr)]">
      <section
        class="flex min-h-0 flex-col"
        data-testid="agent-create-conversation"
        aria-label={$t("agent_create.conversation_title")}
      >
        <h3 class="m-0 pb-2 text-body-sm font-semibold text-foreground">
          {$t("agent_create.conversation_title")}
        </h3>
        <div class="flex min-h-0 flex-1 flex-col overflow-hidden rounded-lg border border-border/60">
          {#if starting}
            <div class="flex flex-1 items-center justify-center gap-2 text-body-xs text-muted-foreground">
              <LoadingSpinner size={14} />
              {$t("agent_create.starting")}
            </div>
          {:else if serverDown !== null}
            <div class="space-y-3 p-4" data-testid="agent-create-server-down">
              <ErrorBanner>
                <p class="font-medium">{$t("agent_create.server_down_title")}</p>
                <p class="text-caption opacity-90">
                  {$t("agent_create.server_down_body", { values: { server: serverDown } })}
                </p>
              </ErrorBanner>
              <div class="flex flex-wrap gap-2">
                <Button variant="outline" size="sm" onclick={() => void start()} data-testid="agent-create-server-retry">
                  {$t("common.retry")}
                </Button>
                {#if !embedded}
                  <Button variant="outline" size="sm" onclick={() => navigateTo("llm")} data-testid="agent-create-open-ai">
                    {$t("agent_create.open_ai")}
                  </Button>
                {/if}
                <Button variant="outline" size="sm" onclick={() => openReview(null)} data-testid="agent-create-server-by-hand">
                  {$t("agent_create.write_by_hand")}
                </Button>
              </div>
            </div>
          {:else if startError !== null}
            <div class="space-y-3 p-4" data-testid="agent-create-start-error">
              <ErrorBanner>
                <p class="font-medium">{$t("agent_create.start_failed")}</p>
                <p class="text-caption opacity-90">{startError}</p>
              </ErrorBanner>
              <div class="flex flex-wrap gap-2">
                <Button variant="outline" size="sm" onclick={() => void start()} data-testid="agent-create-retry">
                  {$t("common.retry")}
                </Button>
                <Button
                  variant="outline"
                  size="sm"
                  onclick={() => openReview(null)}
                  data-testid="agent-create-by-hand"
                >
                  {$t("agent_create.write_by_hand")}
                </Button>
              </div>
            </div>
          {:else if sessionId}
            <ChatConversation {sessionId} onclose={() => undefined} embedded={true} hideConfig={true} />
          {/if}
        </div>
      </section>
      <div class="min-h-0 overflow-y-auto">
        <CreateAgentDraft {draft} {answering} />
      </div>
    </div>
  {/if}
</div>
