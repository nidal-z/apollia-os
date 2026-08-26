<script lang="ts">
  /**
   * Inline 1-click chat creation picker.
   *
   * Replaces the multi-step modal flow with an auto-focused textarea plus
   * clickable agent cards. Opens inline (no full-screen modal),
   * traps focus within its root, and persists expansion state to
   * `localStorage` so power users keep their preferred layout.
   */
  import { onMount, tick } from "svelte";
  import { t } from "svelte-i18n";
  import {
    X,
    ChevronDown,
    ChevronRight,
    Sparkles,
    MessageSquare,
    FolderOpen,
    Wand2,
    Loader2,
    Undo2,
  } from "lucide-svelte";
  import {
    InputRewriter,
    fetchWorkContext,
    type RewriteFallback,
  } from "$lib/chat/rewriteInput";
  import { Button } from "$lib/components/ui/button";
  import { Select } from "$lib/components/ui/select";
  import { LoadingSpinner } from "$lib/components/feedback";
  import type {
    ChatSessionSummary,
    CreateSessionRequest,
  } from "$lib/types";
  import { agents } from "$lib/stores/sse";
  import { listAgents } from "$lib/ipc/connections";
  import { listProjects, createChatSession } from "$lib/ipc/projects";
  import { sendChatMessage } from "$lib/ipc/chat";
  import {
    agentStatuses,
    startAgentStatusPolling,
    type AgentLiveStatus,
  } from "$lib/stores/agentStatus";
  import { triggerAutoName } from "$lib/chat/autoName";
  import { setPlanMode, getPlanModeDefault } from "$lib/ipc/planMode";
  import { projects } from "$lib/stores/projects";
  import AgentStatusCard from "./AgentStatusCard.svelte";
  import EmptyAgentsState from "./EmptyAgentsState.svelte";
  import { focusTrap } from "$lib/shortcuts/focusTrap";
  import { Textarea } from "$lib/components/ui/textarea";

  interface Props {
    /** Initial project binding for the created session. */
    defaultProjectId?: string | null;
    /** Called once the backend returns the created session. */
    oncreated: (session: ChatSessionSummary) => void;
    /** Called when the user dismisses the picker (Esc or close button). */
    onclose: () => void;
  }

  let { defaultProjectId = null, oncreated, onclose }: Props = $props();

  // ─── State ────────────────────────────────────────────────────────────────
  // Focus management is owned by `use:focusTrap` on the root - no DOM
  // refs needed beyond what the action grabs internally.

  let prompt = $state("");
  let creating = $state(false);
  // Surfaces a stable, human-readable reason when session creation fails. The
  // most common case is a missing LLM backend: `create_chat_session` rejects
  // with `NoLlmConfigured` for Libre/Companion mode, and this picker is opened
  // fire-and-forget, so without this the rejection was an unhandled console
  // error with no user feedback.
  let createError = $state<string | null>(null);

  // ── Rewrite input ──────────────────────────────────────────────────────────
  const rewriter = new InputRewriter();
  let isRewriting = $state(false);
  // Mirrors `rewriter.restoreOriginal() !== null` for the template: the class
  // holds plain fields, so a rune has to carry the "a rewrite happened" fact to
  // the restore button.
  let canRestore = $state(false);
  // Whether the free chat starts in plan mode. Seeded from the runtime default
  // so the picker reflects the configured behavior, and applied before the
  // first message so that turn already runs under the plan gate.
  let startInPlanMode = $state(false);
  // Stored as a string ("" == no project) to match the native <Select>'s
  // value model used by the DS Select component, which expects `bind:value`
  // on a string.
  let selectedProjectId = $state<string>(defaultProjectId ?? "");

  const EXPANDED_STORAGE_KEY = "apollia.quickpicker.expanded";
  interface ExpandedState {
    agents: boolean;
  }
  const DEFAULT_EXPANDED: ExpandedState = { agents: true };
  let expanded = $state<ExpandedState>(readExpanded());

  function readExpanded(): ExpandedState {
    if (typeof localStorage === "undefined") return DEFAULT_EXPANDED;
    try {
      const raw = localStorage.getItem(EXPANDED_STORAGE_KEY);
      if (!raw) return DEFAULT_EXPANDED;
      const parsed = JSON.parse(raw) as Partial<ExpandedState>;
      return { ...DEFAULT_EXPANDED, ...parsed };
    } catch {
      return DEFAULT_EXPANDED;
    }
  }

  function persistExpanded(next: ExpandedState): void {
    try {
      localStorage.setItem(EXPANDED_STORAGE_KEY, JSON.stringify(next));
    } catch {
      // Quota or private mode - degrade silently.
    }
  }

  function toggleSection(key: keyof ExpandedState): void {
    expanded = { ...expanded, [key]: !expanded[key] };
    persistExpanded(expanded);
  }

  // ─── Lifecycle ────────────────────────────────────────────────────────────
  // Focus management (initial autofocus + restore on close) is owned by
  // `use:focusTrap` on the root - this hook only seeds backend state.
  onMount(() => {
    void listAgents()
      .then((list) => agents.set(list))
      .catch(() => { /* SSE will eventually populate */ });
    // Seed the projects store so the linker is populated even when the
    // user has not visited the Projects route yet in this session.
    void listProjects()
      .then((list) => projects.set(list))
      .catch(() => { /* link selector will just stay empty */ });
    void getPlanModeDefault()
      .then((on) => {
        startInPlanMode = on;
      })
      .catch(() => { /* keep the default-off toggle */ });
    const stopPolling = startAgentStatusPolling();
    return () => {
      stopPolling();
    };
  });

  $effect(() => {
    // Reset rewriter when prompt is cleared
    if (prompt.trim() === "") {
      rewriter.reset();
      canRestore = false;
    }
  });

  // ─── Derived ──────────────────────────────────────────────────────────────
  const visibleAgents = $derived(
    $agents.filter((a) => a.agent_type !== "system"),
  );
  const hasAgents = $derived(visibleAgents.length > 0);

  function statusOf(name: string): AgentLiveStatus {
    return ($agentStatuses[name] ?? "offline") as AgentLiveStatus;
  }

  // ─── Actions ──────────────────────────────────────────────────────────────

  /// Maps a `create_chat_session` rejection to a stable, human-readable
  /// message. The backend rejects Libre/Companion creation with "no LLM
  /// configured" when no model is set up; surface that as actionable guidance
  /// rather than a raw error string.
  function describeCreateError(err: unknown): string {
    const raw = err instanceof Error ? err.message : String(err);
    if (raw.toLowerCase().includes("no llm configured")) {
      return $t("chat.quickpicker.error_no_llm");
    }
    return $t("chat.quickpicker.error_generic", { values: { error: raw } });
  }

  /**
   * The sentence the operator reads when no rewrite happened.
   *
   * Same four situations as the chat composer, same rule: only the first two
   * mean the engine is missing.
   */
  function rewriteFallbackMessage(fallback: RewriteFallback): string {
    switch (fallback) {
      case "noRouter":
      case "noBackend":
        return $t("chat.rewrite.error_no_llm");
      case "callFailed":
        return $t("chat.rewrite.error_call_failed");
      case "emptyAnswer":
        return $t("chat.rewrite.error_empty_answer");
    }
  }

  async function handleRewrite(): Promise<void> {
    // GIVEN: rewrite already in progress or empty prompt
    if (isRewriting || prompt.trim() === "") return;

    // WHEN: rewrite triggered
    isRewriting = true;
    createError = null;
    try {
      // THEN: fetch Work context from profile
      const workContext = await fetchWorkContext();

      // THEN: call rewriter
      const outcome = await rewriter.rewrite(prompt, workContext);

      // WHEN: no rewrite happened, whichever of the four reasons stopped it
      // THEN: name that reason and leave the prompt as it was
      if (outcome.fallback !== null) {
        createError = rewriteFallbackMessage(outcome.fallback);
        return;
      }

      // WHEN: rewrite succeeds
      // THEN: update prompt
      prompt = outcome.text;
      canRestore = rewriter.restoreOriginal() !== null;
      await tick();
    } catch (err) {
      // WHEN: error occurred
      // THEN: set error message
      createError = describeCreateError(err);
    } finally {
      isRewriting = false;
    }
  }

  async function handleRestoreOriginal(): Promise<void> {
    // GIVEN: a rewrite has been applied at least once
    const original = rewriter.restoreOriginal();
    if (original === null) return;

    // WHEN: the operator asks for the text they typed
    // THEN: put it back, and leave the rewriter armed so a further rewrite
    // still departs from that same original
    prompt = original;
    createError = null;
    await tick();
  }

  async function createFreeChat(initialPrompt?: string, tools?: string[]): Promise<void> {
    if (creating) return;
    creating = true;
    createError = null;
    try {
      const request: CreateSessionRequest = {
        mode: "libre",
        project_id: selectedProjectId || undefined,
        tools,
      };
      const session = await createChatSession(request);
      // Enable plan mode before the first message is sent, so that opening turn
      // already runs under the plan gate. Awaited so it lands before the send.
      if (startInPlanMode) {
        await setPlanMode(session.id, true).catch(() => {
          /* non-fatal: the chip can still toggle it after creation */
        });
      }
      if (initialPrompt && initialPrompt.trim().length > 0) {
        // Kick off auto-naming before send_chat_message so the title LLM call
        // runs concurrently with the agent run - title typically lands first.
        triggerAutoName(session.id, initialPrompt);
        await sendChatMessage(session.id, initialPrompt.trim()).catch(() => { /* message can be retried manually */ });
      }
      oncreated(session);
    } catch (err) {
      // Keep the picker open and show why creation failed (e.g. no LLM
      // configured) instead of letting the fire-and-forget caller drop the
      // rejection into the console.
      createError = describeCreateError(err);
    } finally {
      creating = false;
    }
  }

  async function createAgentChat(agentName: string): Promise<void> {
    if (creating) return;
    creating = true;
    createError = null;
    try {
      const request: CreateSessionRequest = {
        mode: "agent",
        agent_name: agentName,
        project_id: selectedProjectId || undefined,
      };
      const session = await createChatSession(request);
      if (prompt.trim().length > 0) {
        triggerAutoName(session.id, prompt);
        await sendChatMessage(session.id, prompt.trim()).catch(() => { /* user can retry */ });
      }
      oncreated(session);
    } catch (err) {
      createError = describeCreateError(err);
    } finally {
      creating = false;
    }
  }

  // ─── Keyboard ─────────────────────────────────────────────────────────────
  // Tab cycling and focus restoration are handled by `use:focusTrap` on
  // the root - this handler only covers Escape (close) and the textarea
  // submit chord.
  function handleRootKeydown(event: KeyboardEvent): void {
    if (event.key === "Escape") {
      event.preventDefault();
      onclose();
    }
  }

  function handleTextareaKeydown(event: KeyboardEvent): void {
    const isSubmit =
      (event.metaKey || event.ctrlKey) && event.key === "Enter";
    if (isSubmit) {
      event.preventDefault();
      if (prompt.trim().length === 0) return;
      void createFreeChat(prompt);
    }
  }
</script>

<div class="glass-card glass-border animate-fade-in rounded-lg p-4" role="dialog" aria-modal="false" aria-label={$t("chat.quickpicker.title")} tabindex="-1" onkeydown={handleRootKeydown} use:focusTrap={{ initialFocus: "textarea" }} data-testid="quickpicker">
  <!-- Header -->
  <div class="mb-3 flex items-center justify-between">
    <div class="flex items-center gap-2">
      <Sparkles size={13} class="text-primary/80" />
      <span class="text-xs font-medium">{$t("chat.quickpicker.title")}</span>
    </div>
    <button
      type="button"
      onclick={onclose}
      class="inline-flex h-6 w-6 items-center justify-center rounded text-muted-foreground/50 transition-colors hover:text-foreground
        focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/40"
      aria-label={$t("a11y.close")}
      data-testid="quickpicker-close"
    >
      <X size={13} />
    </button>
  </div>

  <!-- Project selector - second entry point for the chat ⇄ project link.
       Hidden only when the user has zero projects; the same action is also
       available later from the conversation header's kebab menu. -->
  {#if $projects.length > 0}
    <div class="mb-3 flex items-center gap-2">
      <label
        for="quickpicker-project-select"
        class="text-caption font-medium text-muted-foreground/80 shrink-0"
      >
        {$t("chat.project_selector")}
      </label>
      <div class="flex-1 min-w-0">
        <Select
          id="quickpicker-project-select"
          icon={FolderOpen}
          size="sm"
          bind:value={selectedProjectId}
          data-testid="quickpicker-project"
          disabled={creating}
        >
          <option value="">{$t("chat.no_project")}</option>
          {#each $projects as proj (proj.id)}
            <option value={proj.id}>{proj.name}</option>
          {/each}
        </Select>
      </div>
    </div>
  {/if}

  <!-- Prompt textarea -->
  <label class="block">
    <span class="sr-only">{$t("chat.quickpicker.prompt_label")}</span>
    <Textarea
      bind:value={prompt}
      onkeydown={handleTextareaKeydown}
      rows={3}
      placeholder={$t("chat.quickpicker.prompt_placeholder")}
      class="w-full resize-none rounded-md border border-border/60 bg-card/60 px-3 py-2 text-sm
        placeholder:text-muted-foreground/60
        focus:border-primary/40 focus:outline-none focus:ring-1 focus:ring-primary/30"
      data-testid="quickpicker-textarea"
    ></Textarea>
  </label>

  <!-- Plan mode toggle: starts this conversation under the plan gate. -->
  <label
    class="mt-2 flex w-fit cursor-pointer select-none items-center gap-2 text-caption text-muted-foreground"
    data-testid="quickpicker-plan-mode-label"
  >
    <input
      type="checkbox"
      bind:checked={startInPlanMode}
      disabled={creating}
      class="h-3.5 w-3.5 rounded border-border accent-primary"
      data-testid="quickpicker-plan-mode"
    />
    {$t("chat.quickpicker.plan_mode")}
  </label>

  <div class="mt-2 flex items-center justify-between">
    <span class="text-micro text-muted-foreground/50">
      {$t("chat.quickpicker.submit_hint")}
    </span>
    <div class="flex items-center gap-2">
      <Button
        size="sm"
        variant="outline"
        onclick={handleRewrite}
        disabled={isRewriting || creating || prompt.trim().length === 0}
        data-testid="quickpicker-rewrite-button"
        class="gap-1.5"
        title={$t("chat.rewrite.button_tooltip")}
      >
        {#if isRewriting}
          <Loader2 size={12} class="animate-spin" />
        {:else}
          <Wand2 size={12} />
        {/if}
        {$t("chat.rewrite.button_tooltip")}
      </Button>
      {#if canRestore}
        <Button
          size="sm"
          variant="ghost"
          onclick={handleRestoreOriginal}
          disabled={isRewriting || creating}
          data-testid="quickpicker-rewrite-restore-button"
          class="gap-1.5"
          title={$t("chat.rewrite.restore_tooltip")}
        >
          <Undo2 size={12} />
          {$t("chat.rewrite.restore_tooltip")}
        </Button>
      {/if}
      <Button
        size="sm"
        onclick={() => createFreeChat(prompt)}
        disabled={creating || prompt.trim().length === 0}
        data-testid="quickpicker-submit"
        class="gap-1.5"
      >
        {#if creating}
          <LoadingSpinner size={12} tone="current" />
        {:else}
          <MessageSquare size={12} />
        {/if}
        {$t("chat.quickpicker.start_free")}
      </Button>
    </div>
  </div>

  {#if createError}
    <div
      class="mt-2 flex items-start gap-2 rounded-md border border-destructive/30 bg-destructive/10 px-3 py-2 text-xs text-destructive"
      role="alert"
      data-testid="quickpicker-error"
    >
      <span>{createError}</span>
    </div>
  {/if}

  <!-- Agents -->
  <section class="mt-4">
    <button
      type="button"
      class="mb-2 inline-flex items-center gap-1.5 text-micro font-semibold uppercase
        tracking-wider text-muted-foreground/60 transition-colors hover:text-foreground
        focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/40 rounded"
      onclick={() => toggleSection("agents")}
      aria-expanded={expanded.agents}
      data-testid="quickpicker-toggle-agents"
    >
      {#if expanded.agents}
        <ChevronDown size={10} />
      {:else}
        <ChevronRight size={10} />
      {/if}
      {$t("chat.quickpicker.agents_section")}
    </button>

    {#if expanded.agents}
      {#if hasAgents}
        <div class="grid grid-cols-1 gap-2 sm:grid-cols-2 lg:grid-cols-3" data-testid="quickpicker-agents">
          {#each visibleAgents as agent (agent.name)}
            <AgentStatusCard
              {agent}
              status={statusOf(agent.name)}
              disabled={creating}
              onselect={createAgentChat}
            />
          {/each}
        </div>
      {:else}
        <!-- Empty state - extracted to EmptyAgentsState for reuse. -->
        <EmptyAgentsState />
      {/if}
    {/if}
  </section>
</div>
