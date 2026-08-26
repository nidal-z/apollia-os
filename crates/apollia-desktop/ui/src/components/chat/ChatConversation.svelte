<script lang="ts">
  import { onMount, onDestroy, tick } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { t } from "svelte-i18n";
  import { LoadingSpinner } from "$lib/components/feedback";
  import CorruptedSessionPanel from "./CorruptedSessionPanel.svelte";
  import {
    createA2ADelegation,
    type RuntimeEventPayload,
  } from "./useA2ADelegation.svelte";
  import {
    currentSession, memoryEntryCount,
    chatConversationStats, globalTokenBuffers,
    clearGlobalBuffer, closeSessionBuffer, removePendingChatApproval,
    getPendingChatApprovalForSession,
    getPendingUserInputForSession, removePendingUserInput,
  } from "$lib/stores/chat";
  import { uiMode } from "$lib/stores/mode";
  import { planModeDefault } from "$lib/stores/planModeSetting";
  import { setPlanMode } from "$lib/ipc/planMode";
  import type {
    ChatSessionDetail,
    ChatMessageView,
    ConversationStatsView,
  } from "$lib/types";
  import { groupMessages } from "$lib/chat/groupMessages";
  import ChatMessageScroller from "./ChatMessageScroller.svelte";
  import type { LiveToolCall } from "./liveToolChain";
  import { parseStream } from "$lib/chat/streamParser";
  import { restoreConversationState } from "./restoreConversationState";
  import {
    handleChatChangedEvent,
    type ChatChangedEvent,
    type ChatChangedPort,
  } from "./chatChangedEvents";
  import { createConversationScroll } from "./useConversationScroll.svelte";
  import { createSubscriptionGuard } from "$lib/utils/subscriptionGuard";
  import { createIdentityGuard } from "$lib/utils/identityGuard";
  import ChatInput from "./ChatInput.svelte";
  import { createSessionActions } from "./useSessionActions.svelte";
  import {
    editAndResendChatMessage,
    getChatSession,
    getConversationStats,
    pauseChatSession,
    regenerateChatResponse,
    sendChatMessage,
  } from "$lib/ipc/chat";
  import { getProfile } from "$lib/ipc/profile";
  import { type PendingAttachment, composeUserPayload } from "$lib/chat/attachments";
  import ChatConfigPanel from "./ChatConfigPanel.svelte";
  import ContextIndicator from "./ContextIndicator.svelte";
  import { refreshSessionMetrics } from "$lib/stores/chatMetrics";
  import InjectedMemorySheet from "../memory/InjectedMemorySheet.svelte";
  import { latestTurnId } from "$lib/stores/thinking";
  import type { InjectedEntry } from "$lib/types";
  import HitlFilesystemModal from "./HitlFilesystemModal.svelte";
  import ChatConversationHeader from "./ChatConversationHeader.svelte";
  import NextStepsPanel from "../common/NextStepsPanel.svelte";
  import { sessionScope } from "$lib/stores/nextSteps";
  import { sessionEndFacts } from "./sessionEndFacts";
  import { classifySessionError } from "$lib/stores/runtimeHealth";
  import { agents } from "$lib/stores/sse";
  import { triggerAutoName } from "$lib/chat/autoName";
  import SessionNotFound from "./SessionNotFound.svelte";
  import AgentUnavailableBanner from "./AgentUnavailableBanner.svelte";
  import { addToast } from "$lib/components/ui/toast/store";

  interface Props {
    sessionId: string;
    onclose: () => void;
    /** When true, hide the header bar (used for embedded contexts like onboarding). */
    embedded?: boolean;
    /** When true, hide the config button in the header. */
    hideConfig?: boolean;
    /**
     * When provided, the Settings button toggles the shell's
     * ContextDrawer instead of opening the internal Sheet. The internal Sheet
     * is suppressed in this mode - the parent owns the config panel.
     */
    onconfigtoggle?: () => void;
    /**
     * When provided (responsive), renders a hamburger button in
     * the header that opens the shell's sessions overlay on small viewports.
     */
    onsessionsopen?: () => void;
    /**
     * When provided (responsive), renders a panel button in the header that
     * toggles the shell's journal / plan rail on viewports below lg.
     */
    oncontextopen?: () => void;
    /**
     * When true, collapses header actions into an overflow
     * menu on narrow viewports (<md).
     */
    collapseActions?: boolean;
    /** Called when the user confirms deletion from the header menu. */
    ondelete?: (sessionId: string) => void;
    /**
     * When true, hides the input bar entirely. Used by onboarding once the
     * wrap-up panel takes over - the user no longer needs to type.
     */
    inputHidden?: boolean;
    /**
     * Called when the user triggers "new chat" from an error state
     * (session not found) - lets the parent open the QuickPicker.
     */
    onnewChat?: () => void;
  }

  let {
    sessionId,
    onclose,
    embedded = false,
    hideConfig = false,
    onconfigtoggle,
    onsessionsopen,
    oncontextopen,
    collapseActions = true,
    ondelete,
    onnewChat,
    inputHidden = false,
  }: Props = $props();

  let messages = $state<ChatMessageView[]>([]);
  let sessionMode = $state<"libre" | "agent">("libre");
  let sessionAgentName = $state<string | null>(null);
  let sessionStatus = $state<"active" | "processing" | "closed">("active");
  let isStreaming = $state(false);
  let isProcessing = $state(false);
  // Last exchange error (e.g. step budget exhausted). Surfaced as a system
  // bubble that survives the session reload in `applySessionDetail`, cleared
  // when the user sends the next message.
  let pendingError = $state<string | null>(null);
  let loading = $state(true);
  let loadError = $state<string | null>(null);
  const loadErrorKind = $derived<"none" | "not_found" | "corrupted" | "other">(
    loadError === null ? "none" : classifySessionError(loadError),
  );
  /** True when the current session references an agent that is no longer
   *  present in the `$agents` list (uninstalled mid-session). */
  const agentUnavailable = $derived(
    sessionMode === "agent" &&
      sessionAgentName !== null &&
      $agents.length > 0 &&
      !$agents.some((a) => a.name === sessionAgentName),
  );
  const scroll = createConversationScroll(() => isStreaming || isProcessing);
  let tokenBuffer = $state("");
  let configOpen = $state(false);
  let injectedSheetOpen = $state(false);
  const currentTurnId = $derived($latestTurnId);
  function openInjectedSheet() {
    injectedSheetOpen = true;
  }
  function handleInjectedEntrySelect(_entry: InjectedEntry) {
    // InsightsPanel navigation wiring lands with; until then,
    // opening the sheet + showing the entry is sufficient for P7.
  }
  let sessionDetail = $state<ChatSessionDetail | null>(null);
  /** Plan-mode state of the active session (drives the header chip + review). */
  let planMode = $state(false);

  // Shared toggle so the header chip and the composer toggle stay in sync.
  // Optimistic: flip immediately, revert + surface on failure.
  async function togglePlanMode(): Promise<void> {
    if (!sessionDetail) return;
    const next = !planMode;
    planMode = next;
    try {
      await setPlanMode(sessionDetail.id, next);
    } catch {
      planMode = !next;
      addToast($t("chat.planMode.toggleError"), "error");
    }
  }
  /** Guards the one-shot inheritance of the global plan-mode default. */
  let planDefaultApplied = $state(false);

  /** Pending tool approval - shown inline when the LLM requests a tool call. */
  let pendingApproval = $state<{
    sessionId: string;
    messageId: string;
    toolCallId: string;
    toolName: string;
    inputPreview: string;
  } | null>(null);
  /** Pending ask_user request - shown inline when the agent needs user input. */
  let pendingUserInput = $state<{
    requestId: string;
    questions: { id: string; question: string; type: "open" | "single_choice" | "multi_choice"; options?: string[]; hint?: string }[];
    context?: string | null;
  } | null>(null);

  let conversationStats = $state<ConversationStatsView | null>(null);

  const inputDisabled = $derived(
    isProcessing || isStreaming || sessionStatus === "closed",
  );

  // summaryText / summarizedCount derivations removed; the
  // banner moved to ContextDrawer > Memory tab (MemoryInjectedPanel).

  const hasCrossSessionRefs = $derived(
    (conversationStats?.cross_sessions_referenced ?? 0) > 0,
  );

  // group consecutive same-role messages within 5 minutes.
  // Memoised by reference: messages array is replaced (not mutated) on every
  // refresh, so $derived recomputes exactly when needed - no thrash during
  // streaming (tokenBuffer changes don't affect the committed messages array).
  const messageGroups = $derived(groupMessages(messages ?? []));

  /**
   * Owns the four runtime subscriptions. Registration below awaits an IPC
   * round-trip first, so a component torn down during that window would
   * otherwise leave its listeners subscribed for the lifetime of the app, one
   * of them still buffering tokens for a session nobody is looking at.
   */
  const actions = createSessionActions(
    () => sessionId,
    () => sessionDetail?.project_id ?? null,
    () => refreshSession(),
  );
  const subscriptions = createSubscriptionGuard();
  const a2a = createA2ADelegation();
  /** Live tool call chain for the current turn, cleared on completion. */
  let liveToolChain = $state<LiveToolCall[]>([]);

  /** Reasoning skin for the live timeline (mirrors the finalized sequence). */
  const liveSkin = $derived<"builder" | "operator">(
    $uiMode === "builder" ? "builder" : "operator",
  );

  // The live chain is rendered in invocation order (no dedup): each tool call
  // is its own row, so the real-time sequence of actions is preserved instead of
  // being collapsed into a "tool_name · ×N" summary that hid the ordering.

  /**
   * What `handleChatChangedEvent` is allowed to touch. Declared next to the
   * state it drives, so a rule of the router reads against one list rather than
   * against the whole component.
   */
  const chatChangedPort: ChatChangedPort = {
    get sessionId() {
      return sessionId;
    },
    closedReasoningCount: () =>
      parseStream(tokenBuffer).filter((b) => b.type === "thinking" && b.closed).length,
    addToolCall(name, reasoningCursor) {
      liveToolChain = [
        ...liveToolChain,
        { name, status: "running", startedAt: Date.now(), reasoningCursor },
      ];
    },
    completeLastToolCall(success) {
      const now = Date.now();
      liveToolChain = liveToolChain.map((step, i) =>
        i === liveToolChain.length - 1 && step.status === "running"
          ? { ...step, status: success ? "done" : "refused", durationMs: now - step.startedAt }
          : step,
      );
    },
    get pendingApprovalToolCallId() {
      return pendingApproval?.toolCallId ?? null;
    },
    setApproval: (approval) => (pendingApproval = approval),
    forgetApproval: (resolvedId) =>
      removePendingChatApproval(sessionId, undefined, resolvedId),
    setUserInput: (input) =>
      (pendingUserInput = input as typeof pendingUserInput),
    forgetUserInput: (requestId) => removePendingUserInput(requestId),
    setStreaming: (streaming) => (isStreaming = streaming),
    setProcessing: (processing) => (isProcessing = processing),
    setPendingError: (detail) => (pendingError = detail),
    showErrorMessage(label) {
      messages = [
        ...(messages ?? []).filter((m) => m.id !== "exchange-error"),
        makeErrorMessage(label),
      ];
    },
    translate: (key, values) => $t(key, values ? { values } : undefined),
    toast: (label) => addToast(label, "error"),
    scrollToBottom: () => scroll.toBottom(),
    finalizeStreaming: () => void finalizeStreaming(),
    refreshSession: () => void refreshSession(),
  };

  onMount(async () => {
    await loadSession();
    restoreGlobalState();
    void actions.loadProjects();

    subscriptions.keep(await listen<{ session_id: string; message_id: string; token: string }>(
      "chat-token",
      (event) => {
        if (event.payload.session_id !== sessionId) return;
        tokenBuffer += event.payload.token;
        isStreaming = true;
        isProcessing = false;
        scroll.toBottom();
      },
    ));

    subscriptions.keep(await listen<RuntimeEventPayload>("runtime-event", (event) => {
      a2a.onLifecycleEvent(event.payload, () => scroll.toBottom());
    }));

    // Sub-agent step events, which only mean something while a delegation runs.
    subscriptions.keep(await listen<RuntimeEventPayload>("runtime-event", (event) => {
      a2a.onStepEvent(event.payload, () => scroll.toBottom());
    }));

    subscriptions.keep(await listen<ChatChangedEvent>("runtime-event", (event) => {
      handleChatChangedEvent(event.payload, chatChangedPort);
    }));
  });

  // Track new messages that land while the user is scrolled up.
  $effect(() => scroll.noteMessageCount(messages.length));

  // Live A2A duration timer - updates every second while delegation is active.
  $effect(() => {
    if (!a2a.running) return;
    const interval = setInterval(() => a2a.tick(), 1000);
    return () => clearInterval(interval);
  });

  // Re-load when sessionId prop changes (switching conversations)
  let previousSessionId = $state(sessionId);
  $effect(() => {
    if (sessionId !== previousSessionId) {
      previousSessionId = sessionId;
      scroll.reset();
      isStreaming = false;
      isProcessing = false;
      tokenBuffer = "";
      a2a.reset();
      liveToolChain = [];
      pendingApproval = null;
      messages = [];
      void loadSession().then(() => restoreGlobalState());
    }
  });

  onDestroy(() => {
    scroll.dispose();
    subscriptions.dispose();
    currentSession.set(null);
    closeSessionBuffer(sessionId);
  });

  // Every read below ends in `applySessionDetail`, which overwrites the whole
  // conversation and pushes it into the global `currentSession` store. Reloading
  // a history takes long enough for the operator to pick another conversation
  // meanwhile, so a read that is no longer aimed at the selected session must
  // write nothing, neither on screen nor in the store.
  const sessionGuard = createIdentityGuard(() => sessionId);

  async function loadSession(): Promise<void> {
    const ticket = sessionGuard.begin();
    loading = true; loadError = null;
    try {
      const detail = await getChatSession(sessionId);
      if (!ticket.current) return;
      applySessionDetail(detail);
    } catch (err: unknown) {
      if (!ticket.current) return;
      loadError = err instanceof Error ? err.message : String(err);
    }
    finally {
      if (ticket.current) { loading = false; await tick(); scroll.toBottom(true); }
    }
  }

  async function refreshSession(): Promise<void> {
    const ticket = sessionGuard.begin();
    try {
      const detail = await getChatSession(sessionId);
      if (!ticket.current) return;
      applySessionDetail(detail); scroll.toBottom();
    } catch { /* Session may have been deleted */ }
  }

  async function finalizeStreaming(): Promise<void> {
    const ticket = sessionGuard.begin();
    await new Promise((r) => setTimeout(r, 80));
    if (!ticket.current) return;
    isStreaming = false; isProcessing = false; tokenBuffer = ""; liveToolChain = [];
    clearGlobalBuffer(sessionId);
    try {
      const detail = await getChatSession(sessionId);
      if (!ticket.current) return;
      applySessionDetail(detail); scroll.toBottom();
    } catch { /* Session may have been deleted */ }
  }

  let stopRequested = $state(false);

  // Stop the in-flight turn (G1). The backend interrupts the ReAct stream at its
  // next token, freezes the partial as the assistant message, and marks the turn
  // paused - so no ChatResponseCompleted event arrives. Reconcile by polling
  // until the session settles out of "processing", then finalize so the frozen
  // partial stays on screen and the composer re-enables. A turn that converges
  // on its own before the stop lands is finalized by the normal event path.
  async function handleStop(): Promise<void> {
    if (stopRequested) return;
    if (!isProcessing && !isStreaming) return;
    stopRequested = true;
    try {
      try {
        await pauseChatSession(sessionId);
      } catch (err) {
        console.error("pause_chat_session failed", err);
      }
      for (let i = 0; i < 40; i++) {
        if (!isProcessing && !isStreaming) break;
        await new Promise((r) => setTimeout(r, 150));
        let status: string;
        try {
          const detail = await getChatSession(sessionId);
          status = detail.status;
        } catch {
          break;
        }
        if (status !== "processing") {
          await finalizeStreaming();
          // A paused turn emits no ChatResponseCompleted, so the metrics
          // refresh that normally follows a turn never fires; refresh
          // explicitly so the context gauge reflects the frozen turn.
          void refreshSessionMetrics(sessionId, true);
          break;
        }
      }
    } finally {
      stopRequested = false;
    }
  }


  // Build the system bubble shown when an exchange fails (id is stable so it is
  // de-duplicated across reloads).
  function makeErrorMessage(text: string): ChatMessageView {
    return {
      id: "exchange-error", role: "system", content: text,
      tool_calls: null, tool_name: null,
      seq: (messages ?? []).length, created_at: new Date().toISOString(),
    };
  }

  function applySessionDetail(detail: ChatSessionDetail): void {
    messages = detail.messages ?? [];
    // The server never persists the exchange error, so re-attach it after a
    // reload until the user sends the next message.
    if (pendingError) messages = [...messages, makeErrorMessage(pendingError)];
    sessionMode = detail.mode;
    sessionAgentName = detail.agent_name;
    sessionStatus = detail.status;
    sessionDetail = detail;
    planMode = detail.plan_mode;
    isProcessing = detail.status === "processing";
    currentSession.set(detail);
    void loadConversationStats();
    void maybeInheritPlanModeDefault(detail);
  }

  /**
   * Applies the global "always plan" default to a brand-new session once.
   *
   * A session is "new" when it has no user message yet. The default is applied
   * a single time per mount through `set_plan_mode`; the per-session header chip
   * overrides it afterwards. Failures are non-fatal: the session simply stays in
   * its current (off) state.
   */
  async function maybeInheritPlanModeDefault(
    detail: ChatSessionDetail,
  ): Promise<void> {
    if (planDefaultApplied) return;
    planDefaultApplied = true;
    const isNew = (detail.messages ?? []).every((m) => m.role !== "user");
    if (!isNew || detail.plan_mode || !$planModeDefault) return;
    try {
      await setPlanMode(detail.id, true);
      planMode = true;
    } catch {
      // Default inheritance is best-effort; leave the session unchanged.
    }
  }

  async function loadConversationStats(): Promise<void> {
    try {
      const stats = await getConversationStats(sessionId);
      conversationStats = stats;
      chatConversationStats.set(stats);
    } catch {
      conversationStats = null;
    }
    try {
      const profile = await getProfile();
      memoryEntryCount.set(profile.entries.length);
    } catch {
      memoryEntryCount.set(0);
    }
  }

  async function handleSend(content: string, attachments: PendingAttachment[] = []): Promise<void> {
    // Auto-name fallback: covers conversations created without an initial
    // prompt (QuickPicker handles the common case before mount). Idempotent
    // via the helper's internal Set, so double-firing is safe.
    const isFirstUserMessage =
      (messages ?? []).every((m) => m.role !== "user") && !sessionDetail?.title;
    if (isFirstUserMessage) {
      triggerAutoName(sessionId, content);
    }

    // Attachments v1: inline small payloads as fenced blocks, reference larger
    // files by absolute path. The backend sees a single user message - the
    // authoritative tool-side ingestion happens via the filesystem HITL flow.
    // The composer refuses anything that fits neither form, so the rendering
    // below is total and never emits a tag without content nor path.
    const payload = composeUserPayload(content, attachments);

    const tempMsg: ChatMessageView = {
      id: `temp-${Date.now()}`, role: "user", content: payload,
      tool_calls: null, tool_name: null,
      seq: (messages ?? []).length, created_at: new Date().toISOString(),
    };
    pendingError = null;
    messages = [...(messages ?? []).filter((m) => m.id !== "exchange-error"), tempMsg];
    isProcessing = true; tokenBuffer = ""; liveToolChain = [];
    await tick(); scroll.toBottom(true);

    try {
      await sendChatMessage(sessionId, payload);
    } catch (err: unknown) {
      isProcessing = false;
      const errMsg: ChatMessageView = {
        id: `error-${Date.now()}`, role: "system",
        content: err instanceof Error ? err.message : String(err),
        tool_calls: null, tool_name: null,
        seq: (messages ?? []).length, created_at: new Date().toISOString(),
      };
      messages = [...(messages ?? []), errMsg]; scroll.toBottom();
    }
  }

  // Regenerate the assistant reply to a turn (G9). Optimistically truncate the
  // local thread back to the user turn, then let the backend replay it; the
  // streaming events render the new answer and finalizeStreaming reloads the
  // authoritative history.
  async function handleRegenerate(messageId: string): Promise<void> {
    if (isProcessing || isStreaming) return;
    const msgs = messages ?? [];
    const idx = msgs.findIndex((m) => m.id === messageId);
    if (idx < 0) return;
    let userIdx = -1;
    for (let i = idx; i >= 0; i--) {
      if (msgs[i]!.role === "user") { userIdx = i; break; }
    }
    if (userIdx < 0) return;
    pendingError = null;
    messages = msgs.slice(0, userIdx + 1);
    isProcessing = true; tokenBuffer = ""; liveToolChain = [];
    await tick(); scroll.toBottom(true);
    try {
      await regenerateChatResponse(sessionId, messageId);
    } catch (err: unknown) {
      isProcessing = false;
      addToast(err instanceof Error ? err.message : String(err), "error");
      void finalizeStreaming();
    }
  }

  // Edit a user message and re-run from it (G10). Truncate the local thread
  // before the edited message, show the new text optimistically, then send it
  // as a fresh turn through the truncate-in-place backend path.
  async function handleEdit(messageId: string, newContent: string): Promise<void> {
    if (isProcessing || isStreaming) return;
    const msgs = messages ?? [];
    const idx = msgs.findIndex((m) => m.id === messageId);
    if (idx < 0) return;
    pendingError = null;
    const kept = msgs.slice(0, idx);
    const tempMsg: ChatMessageView = {
      id: `temp-${Date.now()}`, role: "user", content: newContent,
      tool_calls: null, tool_name: null,
      seq: kept.length, created_at: new Date().toISOString(),
    };
    messages = [...kept, tempMsg];
    isProcessing = true; tokenBuffer = ""; liveToolChain = [];
    await tick(); scroll.toBottom(true);
    try {
      await editAndResendChatMessage(sessionId, messageId, newContent);
    } catch (err: unknown) {
      isProcessing = false;
      addToast(err instanceof Error ? err.message : String(err), "error");
      void finalizeStreaming();
    }
  }

  // --- Slash-command plumbing --------------------------------
  const lastUserMessageText = $derived.by(() => {
    const msgs = messages ?? [];
    for (let i = msgs.length - 1; i >= 0; i--) {
      const m = msgs[i]!;
      if (m.role === "user" && m.content.trim()) return m.content;
    }
    return null;
  });

  function handleDeleteSession(): void {
    if (!ondelete) return;
    ondelete(sessionId);
  }

  /**
   * Restore streaming & approval state from global stores when the component
   * (re-)mounts or the session switches.  This handles the case where the
   * backend continued streaming while the user was on a different page.
   */
  function restoreGlobalState(): void {
    const patch = restoreConversationState({
      sessionId,
      buffers: $globalTokenBuffers,
      sessionStatus,
      approval: getPendingChatApprovalForSession(sessionId),
      userInput: getPendingUserInputForSession(sessionId),
      hasUserInput: pendingUserInput !== null,
    });
    if (patch.tokenBuffer !== undefined) tokenBuffer = patch.tokenBuffer;
    if (patch.isStreaming !== undefined) isStreaming = patch.isStreaming;
    if (patch.isProcessing !== undefined) isProcessing = patch.isProcessing;
    if (patch.approval) pendingApproval = patch.approval;
    if (patch.userInput) pendingUserInput = patch.userInput;
    if (patch.scroll) scroll.toBottom();
  }

  // ── Next Steps ─────────────────────────────────────────
  // Rendered only when the session is closed - acts as a debrief panel.
  const sessionEndScope = $derived(sessionScope(sessionId));
  const nextStepsFacts = $derived(sessionEndFacts(messages, $memoryEntryCount ?? 0));
</script>

<div class="flex h-full flex-col" data-testid="chat-conversation">
  <!-- Two-level header (hidden in embedded mode). -->
  {#if !embedded}
    <ChatConversationHeader
      session={sessionDetail}
      stats={conversationStats}
      {sessionMode}
      {sessionAgentName}
      {sessionStatus}
      {hideConfig}
      {collapseActions}
      renameTrigger={actions.renameTrigger}
      onclose={onclose}
      onconfigtoggle={onconfigtoggle ? onconfigtoggle : () => (configOpen = true)}
      {onsessionsopen}
      {oncontextopen}
      onrename={(title) => void actions.rename(title)}
      ondelete={handleDeleteSession}
      linkedProject={actions.linkedProject}
      availableProjects={actions.availableProjects}
      onlink={(projectId) => void actions.link(projectId)}
      onprojectopen={() => actions.openProjects()}
      {planMode}
      onplanmodechange={(enabled) => {
        planMode = enabled;
        void refreshSession();
      }}
    />
  {/if}

  <!-- Context indicator: removed from header. The pill
       now lives in the Metrics tab of the ContextDrawer, and a mini variant
       is rendered below the input (see footer near ChatInput). -->
  {#if sessionId}
    <ContextIndicator {sessionId} variant="footer" onclick={openInjectedSheet} />
  {/if}

  <InjectedMemorySheet
    open={injectedSheetOpen}
    turnId={currentTurnId}
    onclose={() => (injectedSheetOpen = false)}
    onentryselect={handleInjectedEntrySelect}
  />

  <!-- Agent disparu inline banner. Rendered above the
       messages so the transcript stays readable. -->
  {#if !loading && loadErrorKind === "none" && agentUnavailable && sessionAgentName}
    <AgentUnavailableBanner agentName={sessionAgentName} />
  {/if}

  <!-- Messages -->
  {#if loading}
    <div class="flex flex-1 items-center justify-center">
      <LoadingSpinner size={16} tone="muted" />
    </div>
  {:else if loadErrorKind === "not_found"}
    <SessionNotFound
      sessionId={sessionId}
      onback={onclose}
      onnewChat={() => (onnewChat ? onnewChat() : onclose())}
    />
  {:else if loadErrorKind === "corrupted"}
    <CorruptedSessionPanel
      {sessionId}
      {loadError}
      ondelete={() => (ondelete ? ondelete(sessionId) : onclose())}
    />
  {:else if loadErrorKind === "other"}
    <div
      class="flex flex-1 items-center justify-center px-6"
      role="alert"
      aria-live="polite"
    >
      <p class="text-xs text-destructive">{loadError}</p>
    </div>
  {:else}
    <ChatMessageScroller
      {sessionId}
      {sessionMode}
      {sessionAgentName}
      {sessionStatus}
      {messageGroups}
      empty={(messages ?? []).length === 0}
      {isStreaming}
      {isProcessing}
      {tokenBuffer}
      {liveToolChain}
      {liveSkin}
      {hasCrossSessionRefs}
      {a2a}
      {scroll}
      {pendingApproval}
      {pendingUserInput}
      onregenerate={handleRegenerate}
      onedit={handleEdit}
    />
  {/if}

  {#if sessionStatus === "closed" && !loading && loadErrorKind === "none"}
    <div class="border-t border-border/20 bg-background/40 px-4 py-3" data-testid="chat-next-steps">
      <NextStepsPanel
        scopeKey={sessionEndScope}
        context="session_end"
        mode={$uiMode === "builder" ? "builder" : "operator"}
        facts={nextStepsFacts}
        title={$t("next_steps.session_title")}
      />
    </div>
  {/if}

  {#if !inputHidden}
    <ChatInput
      disabled={inputDisabled}
      busy={isProcessing || isStreaming}
      onstop={handleStop}
      onsend={handleSend}
      lastUserMessage={lastUserMessageText}
      oncommand={(cmdId) => void actions.runSlashCommand(cmdId)}
      {planMode}
      onplantoggle={togglePlanMode}
      planDisabled={sessionStatus === "closed"}
    />
  {/if}
</div>

{#if !embedded && !onconfigtoggle}
<ChatConfigPanel
  open={configOpen}
  session={sessionDetail}
  onclose={() => (configOpen = false)}
  onupdated={() => void refreshSession()}
/>
{/if}

<HitlFilesystemModal {sessionId} />
