<script lang="ts">
  import { onMount, onDestroy, tick } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { t } from "svelte-i18n";
  import { X, MessageSquare, Link, Zap, BrainCircuit, Check } from "lucide-svelte";
  import { LoadingSpinner } from "$lib/components/feedback";
  import { Spinner } from "$lib/components/ui/progress";
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
    ProjectSummary,
    UserProfileView,
  } from "$lib/types";
  import MessageGroup from "./MessageGroup.svelte";
  import { groupMessages } from "$lib/chat/groupMessages";
  import ChatInput from "./ChatInput.svelte";
  import { save as saveDialog } from "@tauri-apps/plugin-dialog";
  import { exportConversation, type ExportFormat } from "$lib/chat/exportConversation";
  import type { PendingAttachment } from "$lib/chat/attachments";
  import StreamingMessage from "./StreamingMessage.svelte";
  import ChatConfigPanel from "./ChatConfigPanel.svelte";
  import ContextIndicator from "./ContextIndicator.svelte";
  import InjectedMemorySheet from "../memory/InjectedMemorySheet.svelte";
  import { latestTurnId } from "$lib/stores/thinking";
  import type { InjectedEntry } from "$lib/types";
  import ApprovalCard from "./ApprovalCard.svelte";
  import AskUserCard from "./AskUserCard.svelte";
  import HitlFilesystemModal from "./HitlFilesystemModal.svelte";
  import ChatConversationHeader from "./ChatConversationHeader.svelte";
  import ScrollToBottomButton from "./ScrollToBottomButton.svelte";
  import NextStepsPanel from "../common/NextStepsPanel.svelte";
  import { sessionScope, type NextStepsFacts } from "$lib/stores/nextSteps";
  import { classifySessionError } from "$lib/stores/runtimeHealth";
  import { agents } from "$lib/stores/sse";
  import { triggerAutoName } from "$lib/chat/autoName";
  import SessionNotFound from "./SessionNotFound.svelte";
  import AgentUnavailableBanner from "./AgentUnavailableBanner.svelte";
  import { AlertOctagon } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
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
  let messagesContainer = $state<HTMLDivElement | undefined>(undefined);
  let userScrolledUp = $state(false);
  /** B.8 - floating "jump to latest" button visibility + unread count. */
  let showScrollToBottom = $state(false);
  let unreadWhileScrolled = $state(0);
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
  /** Guards the one-shot inheritance of the global plan-mode default (AC-2). */
  let planDefaultApplied = $state(false);

  /** Pending tool approval - shown inline when the LLM requests a tool call. */
  let pendingApproval = $state<{
    sessionId: string;
    messageId: string;
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

  let unlistenToken: UnlistenFn | undefined;
  let unlistenChanged: UnlistenFn | undefined;
  let unlistenA2A: UnlistenFn | undefined;
  let unlistenTaskChanged: UnlistenFn | undefined;
  let activeToolName = $state<string | null>(null);
  /** Non-null while an A2A delegation is in progress. */
  let activeA2A = $state<{ target: string; skill_id: string } | null>(null);
  /** Steps reported by the sub-agent during A2A delegation. */
  let a2aSteps = $state<
    { step_id: string; step_num: number; total: number; desc: string; status: "running" | "done" | "failed"; durationMs?: number }[]
  >([]);
  /** Guard trigger message from A2A guardrails. */
  let a2aGuardMessage = $state<string | null>(null);
  /** Start time of current A2A delegation for live duration display. */
  let a2aStartTime = $state<number | null>(null);
  /** Elapsed seconds of current A2A delegation (updated every second). */
  let a2aElapsed = $state<number>(0);
  /** Live tool call chain for the current LLM turn - cleared on response completion. */
  let liveToolChain = $state<
    { name: string; status: "running" | "done" | "refused"; startedAt: number; durationMs?: number }[]
  >([]);

  // Dedup tool-name entries in the live chain.  Keep the latest status
  // + cumulative duration, track invocation count so repeat calls surface
  // compactly as "tool_name · ×N".
  const groupedLiveToolChain = $derived.by(() => {
    const byName = new Map<
      string,
      { name: string; status: "running" | "done" | "refused"; count: number; totalDurationMs: number }
    >();
    for (const step of liveToolChain) {
      const existing = byName.get(step.name);
      if (existing) {
        existing.status = step.status;
        existing.count += 1;
        existing.totalDurationMs += step.durationMs ?? 0;
      } else {
        byName.set(step.name, {
          name: step.name,
          status: step.status,
          count: 1,
          totalDurationMs: step.durationMs ?? 0,
        });
      }
    }
    return Array.from(byName.values());
  });

  onMount(async () => {
    await loadSession();
    restoreGlobalState();
    void loadAvailableProjects();

    unlistenToken = await listen<{ session_id: string; message_id: string; token: string }>(
      "chat-token",
      (event) => {
        if (event.payload.session_id !== sessionId) return;
        activeToolName = null;
        tokenBuffer += event.payload.token;
        isStreaming = true;
        isProcessing = false;
        scrollToBottom();
      },
    );

    unlistenA2A = await listen<{ category: string; event_type: string; payload: Record<string, unknown> }>(
      "runtime-event",
      (event) => {
        if (event.payload.category !== "a2a") return;
        const evt = event.payload;
        if (evt.event_type === "A2AInvocationStarted") {
          const p = evt.payload as { caller?: string; target?: string; skill_id?: string };
          if (p.caller === "chat-libre") {
            activeA2A = { target: p.target ?? "", skill_id: p.skill_id ?? "" };
            a2aSteps = [];
            a2aGuardMessage = null;
            a2aStartTime = Date.now();
            a2aElapsed = 0;
            scrollToBottom();
          }
        } else if (evt.event_type === "A2AInvocationCompleted") {
          const p = evt.payload as { status?: string; duration_ms?: number };
          // Brief delay to show final status before clearing
          const finalStatus = p.status ?? "completed";
          const finalDuration = p.duration_ms;
          if (finalStatus === "failed" && activeA2A) {
            a2aGuardMessage = `Delegation failed (${finalDuration ? `${finalDuration}ms` : "unknown duration"})`;
          }
          setTimeout(() => {
            activeA2A = null;
            a2aSteps = [];
            a2aGuardMessage = null;
            a2aStartTime = null;
            a2aElapsed = 0;
          }, finalStatus === "failed" ? 2000 : 300);
        } else if (evt.event_type === "A2AGuardTriggered") {
          const p = evt.payload as { detail?: string; guard_type?: string };
          a2aGuardMessage = p.detail ?? `Guard: ${p.guard_type}`;
          scrollToBottom();
        }
      },
    );

    // Listen for sub-agent step events during A2A delegation.
    unlistenTaskChanged = await listen<{ category: string; event_type: string; payload: Record<string, unknown> }>(
      "runtime-event",
      (event) => {
        if (event.payload.category !== "task-changed") return;
        if (!activeA2A) return;
        const evt = event.payload;

        if (evt.event_type === "StepStarted") {
          const p = evt.payload as { step_id?: string; step_num?: number; total?: number; desc?: string };
          a2aSteps = [
            ...a2aSteps,
            {
              step_id: p.step_id ?? `s${a2aSteps.length}`,
              step_num: p.step_num ?? a2aSteps.length + 1,
              total: p.total ?? 0,
              desc: p.desc ?? "",
              status: "running",
            },
          ];
          scrollToBottom();
        } else if (evt.event_type === "StepCompleted") {
          const p = evt.payload as { step_id?: string; duration_ms?: number };
          a2aSteps = a2aSteps.map((s) =>
            s.step_id === p.step_id ? { ...s, status: "done" as const, durationMs: p.duration_ms } : s,
          );
        } else if (evt.event_type === "StepFailed") {
          const p = evt.payload as { step_id?: string; error?: string };
          a2aSteps = a2aSteps.map((s) =>
            s.step_id === p.step_id ? { ...s, status: "failed" as const } : s,
          );
        } else if (evt.event_type === "StepExecuted") {
          // Direct-mode agents emit StepExecuted instead of StepStarted/Completed.
          const p = evt.payload as { step?: number; tool?: string };
          a2aSteps = [
            ...a2aSteps,
            {
              step_id: `step-${p.step ?? a2aSteps.length}`,
              step_num: (p.step ?? a2aSteps.length) + 1,
              total: 0,
              desc: p.tool ?? "step",
              status: "done",
            },
          ];
          scrollToBottom();
        }
      },
    );

    unlistenChanged = await listen<{ category: string; event_type: string; payload: Record<string, unknown> }>(
      "runtime-event",
      (event) => {
        if (event.payload.category !== "chat-changed") return;
        const evt = event.payload;

        if (evt.event_type === "ChatToolCallStarted") {
          const p = evt.payload as { session_id?: string; tool_name?: string };
          if (p.session_id === sessionId) {
            activeToolName = p.tool_name ?? null;
            liveToolChain = [
              ...liveToolChain,
              { name: p.tool_name ?? "?", status: "running", startedAt: Date.now() },
            ];
            pendingApproval = null;
            scrollToBottom();
          }
          return;
        }
        if (evt.event_type === "ChatToolCallCompleted") {
          const p = evt.payload as { session_id?: string; success?: boolean };
          if (p.session_id === sessionId) {
            activeToolName = null;
            const now = Date.now();
            liveToolChain = liveToolChain.map((step, i) =>
              i === liveToolChain.length - 1 && step.status === "running"
                ? { ...step, status: p.success === false ? "refused" : "done", durationMs: now - step.startedAt }
                : step,
            );
          }
          return;
        }
        if (evt.event_type === "ChatApprovalRequired") {
          // Payload is externally-tagged serde: { ChatApprovalRequired: { session_id, ... } }
          const inner = (evt.payload as Record<string, unknown>)?.ChatApprovalRequired as
            { session_id?: string; message_id?: string; tool_name?: string; prompt?: string } | undefined;
          const p = inner ?? evt.payload as { session_id?: string; message_id?: string; tool_name?: string; prompt?: string };
          if (!p.session_id || p.session_id === sessionId) {
            isStreaming = false;
            pendingApproval = {
              sessionId: sessionId,
              messageId: p.message_id ?? "",
              toolName: p.tool_name ?? "",
              inputPreview: p.prompt ?? "",
            };
            scrollToBottom();
          }
          return;
        }
        if (evt.event_type === "ChatApprovalResolved" || evt.event_type === "ChatApprovalTimeout") {
          pendingApproval = null;
          removePendingChatApproval(sessionId);
          return;
        }
        if (evt.event_type === "ChatUserInputRequired") {
          const inner = (evt.payload as Record<string, unknown>)?.ChatUserInputRequired as
            { request_id?: string; session_id?: string; questions_json?: string; context?: string } | undefined;
          const p = inner ?? evt.payload as { request_id?: string; session_id?: string; questions_json?: string; context?: string };
          if (!p.session_id || p.session_id === sessionId || p.session_id === "") {
            isStreaming = false;
            try {
              const questions = JSON.parse(p.questions_json ?? "[]");
              pendingUserInput = {
                requestId: p.request_id ?? "",
                questions,
                context: p.context ?? null,
              };
            } catch {
              console.warn("Failed to parse ask_user questions:", p.questions_json);
            }
            scrollToBottom();
          }
          return;
        }
        if (evt.event_type === "ChatUserInputResolved") {
          const inner = (evt.payload as Record<string, unknown>)?.ChatUserInputResolved as
            { request_id?: string } | undefined;
          const p = inner ?? evt.payload as { request_id?: string };
          pendingUserInput = null;
          if (p.request_id) removePendingUserInput(String(p.request_id));
          return;
        }
        if (evt.event_type === "ChatResponseCompleted") {
          pendingApproval = null;
          void finalizeStreaming();
          return;
        }
        void refreshSession();
      },
    );
  });

  // Track new messages that land while the user is scrolled up (B.8).
  let lastSeenMessageCount = $state(0);
  $effect(() => {
    const count = messages.length;
    if (userScrolledUp && count > lastSeenMessageCount) {
      unreadWhileScrolled += count - lastSeenMessageCount;
    }
    lastSeenMessageCount = count;
  });

  // Live A2A duration timer - updates every second while delegation is active.
  $effect(() => {
    if (!a2aStartTime) return;
    const interval = setInterval(() => {
      if (a2aStartTime) {
        a2aElapsed = Math.round((Date.now() - a2aStartTime) / 1000);
      }
    }, 1000);
    return () => clearInterval(interval);
  });

  // Re-load when sessionId prop changes (switching conversations)
  let previousSessionId = $state(sessionId);
  $effect(() => {
    if (sessionId !== previousSessionId) {
      previousSessionId = sessionId;
      unreadWhileScrolled = 0;
      showScrollToBottom = false;
      userScrolledUp = false;
      lastSeenMessageCount = 0;
      isStreaming = false;
      isProcessing = false;
      tokenBuffer = "";
      activeToolName = null;
      activeA2A = null;
      a2aSteps = [];
      a2aGuardMessage = null;
      a2aStartTime = null;
      a2aElapsed = 0;
      liveToolChain = [];
      pendingApproval = null;
      messages = [];
      void loadSession().then(() => restoreGlobalState());
    }
  });

  onDestroy(() => {
    unlistenToken?.();
    unlistenChanged?.();
    unlistenA2A?.();
    unlistenTaskChanged?.();
    currentSession.set(null);
    closeSessionBuffer(sessionId);
  });

  async function loadSession(): Promise<void> {
    loading = true; loadError = null;
    try {
      const detail = await invoke<ChatSessionDetail>("get_chat_session", { sessionId });
      applySessionDetail(detail);
    } catch (err: unknown) { loadError = err instanceof Error ? err.message : String(err); }
    finally { loading = false; await tick(); scrollToBottom(true); }
  }

  async function refreshSession(): Promise<void> {
    try {
      const detail = await invoke<ChatSessionDetail>("get_chat_session", { sessionId });
      applySessionDetail(detail); scrollToBottom();
    } catch { /* Session may have been deleted */ }
  }

  async function finalizeStreaming(): Promise<void> {
    await new Promise((r) => setTimeout(r, 80));
    isStreaming = false; isProcessing = false; tokenBuffer = ""; liveToolChain = [];
    clearGlobalBuffer(sessionId);
    try {
      const detail = await invoke<ChatSessionDetail>("get_chat_session", { sessionId });
      applySessionDetail(detail); scrollToBottom();
    } catch { /* Session may have been deleted */ }
  }


  function applySessionDetail(detail: ChatSessionDetail): void {
    messages = detail.messages ?? [];
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
   * Applies the global "always plan" default to a brand-new session once (AC-2).
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
      const stats = await invoke<ConversationStatsView>("get_conversation_stats", { sessionId });
      conversationStats = stats;
      chatConversationStats.set(stats);
    } catch {
      conversationStats = null;
    }
    try {
      const profile = await invoke<UserProfileView>("get_profile");
      memoryEntryCount.set(profile.entries.length);
    } catch {
      memoryEntryCount.set(0);
    }
  }

  function scrollToBottom(force = false): void {
    if (!force && userScrolledUp) return;
    requestAnimationFrame(() => {
      if (messagesContainer) {
        messagesContainer.scrollTo({ top: messagesContainer.scrollHeight, behavior: force ? "instant" : "smooth" });
      }
    });
  }

  /** Scroll observer (B.8). Shows floating button when the user
   *  scrolls more than ~200px above the bottom, resets unread counter on catch-up. */
  function handleScroll(): void {
    if (!messagesContainer) return;
    const { scrollTop, scrollHeight, clientHeight } = messagesContainer;
    const distanceFromBottom = scrollHeight - scrollTop - clientHeight;
    userScrolledUp = distanceFromBottom > 60;
    showScrollToBottom = distanceFromBottom > 200;
    if (!userScrolledUp) {
      unreadWhileScrolled = 0;
    }
  }

  function jumpToLatest(): void {
    userScrolledUp = false;
    unreadWhileScrolled = 0;
    showScrollToBottom = false;
    scrollToBottom(true);
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
    let payload = content;
    if (attachments.length > 0) {
      const parts: string[] = [];
      if (content.trim()) parts.push(content);
      for (const att of attachments) {
        if (att.base64 && att.kind !== "image") {
          parts.push(`\n<attachment name="${att.name}" mime="${att.mime}" size="${att.size}">\n${decodeBase64Utf8(att.base64)}\n</attachment>`);
        } else if (att.absolutePath) {
          parts.push(`\n<attachment name="${att.name}" path="${att.absolutePath}" size="${att.size}" />`);
        } else if (att.kind === "image" && att.base64) {
          parts.push(`\n<attachment name="${att.name}" mime="${att.mime}" encoding="base64">${att.base64}</attachment>`);
        } else {
          parts.push(`\n<attachment name="${att.name}" size="${att.size}" />`);
        }
      }
      payload = parts.join("");
    }

    const tempMsg: ChatMessageView = {
      id: `temp-${Date.now()}`, role: "user", content: payload,
      tool_calls: null, tool_name: null,
      seq: (messages ?? []).length, created_at: new Date().toISOString(),
    };
    messages = [...(messages ?? []), tempMsg];
    isProcessing = true; tokenBuffer = ""; liveToolChain = [];
    await tick(); scrollToBottom(true);

    try {
      await invoke<string>("send_chat_message", { sessionId, content: payload });
    } catch (err: unknown) {
      isProcessing = false;
      const errMsg: ChatMessageView = {
        id: `error-${Date.now()}`, role: "system",
        content: err instanceof Error ? err.message : String(err),
        tool_calls: null, tool_name: null,
        seq: (messages ?? []).length, created_at: new Date().toISOString(),
      };
      messages = [...(messages ?? []), errMsg]; scrollToBottom();
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

  function decodeBase64Utf8(b64: string): string {
    try {
      const bin = atob(b64);
      const bytes = Uint8Array.from(bin, (c) => c.charCodeAt(0));
      return new TextDecoder("utf-8", { fatal: false }).decode(bytes);
    } catch {
      return b64;
    }
  }

  async function handleSlashCommand(cmdId: "export" | "rename"): Promise<void> {
    switch (cmdId) {
      case "rename": {
        const title = prompt($t("chat.rename_placeholder"), sessionDetail?.title ?? "");
        if (title !== null && title.trim()) {
          await handleRename(title.trim());
        }
        return;
      }
      case "export":
        await exportCurrentSession("markdown-with-tools");
        return;
    }
  }

  async function exportCurrentSession(format: ExportFormat): Promise<void> {
    try {
      const detail = await invoke<ChatSessionDetail>("get_chat_session", { sessionId });
      const { content, filename, mime } = exportConversation(detail, format);
      const dest = await saveDialog({
        defaultPath: filename,
        filters: [{
          name: format === "json" ? "JSON" : "Markdown",
          extensions: [format === "json" ? "json" : "md"],
        }],
      });
      if (!dest) return;
      await invoke("export_conversation", { destPath: dest, content, mime });
    } catch (err) {
      addToast(err instanceof Error ? err.message : String(err), "error");
    }
  }

  function handleDeleteSession(): void {
    if (!ondelete) return;
    ondelete(sessionId);
  }

  async function handleRename(title: string): Promise<void> {
    try {
      await invoke("rename_chat_session", { sessionId, title });
      void refreshSession();
    } catch (err) {
      console.warn("rename_chat_session failed", err);
    }
  }

  // ─── Project linking ─────────────────────────────────────────────────────

  let availableProjects = $state<ProjectSummary[]>([]);

  const linkedProject = $derived<ProjectSummary | null>(
    (() => {
      const pid = sessionDetail?.project_id ?? null;
      if (!pid) return null;
      return availableProjects.find((p) => p.id === pid) ?? null;
    })(),
  );

  async function loadAvailableProjects(): Promise<void> {
    try {
      availableProjects = await invoke<ProjectSummary[]>("list_projects");
    } catch (err) {
      console.warn("list_projects failed", err);
    }
  }

  async function handleLinkProject(projectId: string | null): Promise<void> {
    try {
      await invoke("link_chat_to_project", {
        sessionId,
        projectId,
      });
      await refreshSession();
      // Refresh the global chat-sessions store so the sidebar chip updates
      // immediately (link_chat_to_project does not emit a runtime event).
      try {
        const updated = await invoke<import("$lib/types").ChatSessionSummary[]>(
          "list_chat_sessions",
        );
        const { chatSessions } = await import("$lib/stores/sse");
        chatSessions.set(updated);
      } catch { /* non-blocking */ }
      if (projectId) {
        const project = availableProjects.find((p) => p.id === projectId);
        addToast(
          $t("chat.project_linked_toast", {
            values: { name: project?.name ?? "" },
          }),
          "success",
        );
      } else {
        addToast($t("chat.project_unlinked_toast"), "success");
      }
    } catch (err) {
      addToast(
        `${$t("chat.project_link_failed")} - ${err instanceof Error ? err.message : String(err)}`,
        "error",
      );
    }
  }

  function handleProjectChipOpen(_projectId: string): void {
    import("$lib/stores/navigation").then((m) => m.navigateTo("projects"));
  }

  async function exportCorruptedSessionRaw(): Promise<void> {
    // The UX contract is "give me something to send to support". We dump
    // whatever the backend was able to surface - even if only the raw
    // error - into a JSON envelope so the user can forward it.
    let raw: unknown = null;
    try {
      raw = await invoke("get_chat_session_raw", { sessionId });
    } catch {
      raw = null;
    }
    const envelope = {
      session_id: sessionId,
      captured_at: new Date().toISOString(),
      load_error: loadError,
      raw,
    };
    try {
      // Browser-native download - works inside the Tauri webview without
      // pulling an extra plugin dependency. The user picks the destination
      // via the OS "Save as" dialog emitted by the anchor click.
      const blob = new Blob([JSON.stringify(envelope, null, 2)], {
        type: "application/json",
      });
      const url = URL.createObjectURL(blob);
      const anchor = document.createElement("a");
      anchor.href = url;
      anchor.download = `apollia-session-${sessionId.slice(0, 8)}-raw.json`;
      document.body.appendChild(anchor);
      anchor.click();
      anchor.remove();
      URL.revokeObjectURL(url);
    } catch (err) {
      console.warn("export corrupted session failed", err);
    }
  }

  function deleteCorruptedSession(): void {
    if (ondelete) ondelete(sessionId);
    else onclose();
  }


  /**
   * Restore streaming & approval state from global stores when the component
   * (re-)mounts or the session switches.  This handles the case where the
   * backend continued streaming while the user was on a different page.
   */
  function restoreGlobalState(): void {
    // Restore accumulated tokens from global buffer
    const buffers = $globalTokenBuffers;
    const bufferedText = buffers[sessionId];
    if (bufferedText) {
      tokenBuffer = bufferedText;
      isStreaming = true;
      isProcessing = false;
      scrollToBottom();
    }

    // Restore pending approval from global store
    const approval = getPendingChatApprovalForSession(sessionId);
    if (approval) {
      pendingApproval = {
        sessionId: approval.sessionId,
        messageId: approval.messageId,
        toolName: approval.toolName,
        inputPreview: approval.inputPreview,
      };
      isStreaming = false;
      scrollToBottom();
    }

    // Restore pending ask_user request from global store
    const userInput = getPendingUserInputForSession(sessionId);
    if (userInput && !pendingUserInput) {
      try {
        const questions = JSON.parse(userInput.questions_json);
        pendingUserInput = {
          requestId: userInput.request_id,
          questions,
          context: userInput.context,
        };
        isStreaming = false;
        scrollToBottom();
      } catch {
        // Malformed questions_json - ignore, will be re-emitted by backend
      }
    }

    // If session is still processing but we have no tokens yet, show processing state
    if (sessionStatus === "processing" && !bufferedText && !approval && !userInput) {
      isProcessing = true;
    }
  }

  // ── Next Steps ─────────────────────────────────────────
  // Rendered only when the session is closed - acts as a debrief panel.
  const sessionEndScope = $derived(sessionScope(sessionId));
  const nextStepsFacts = $derived<NextStepsFacts>({
    recentMessages: messages
      .slice(-8)
      .filter((m) => m.role === "user" || m.role === "assistant")
      .map((m) => `${m.role}: ${m.content.slice(0, 240)}`),
    toolsUsed: Array.from(
      new Set(
        messages
          .flatMap((m) => m.tool_calls ?? [])
          .map((tc) => tc.tool_name),
      ),
    ).slice(0, 12),
    memoriesCreated: 0,
    memoriesRecalled: $memoryEntryCount ?? 0,
    inboxPending: 0,
    tasksFailed: 0,
    tasksCompleted: 0,
    automationsFailing: 0,
    signals: [],
  });
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
      onclose={onclose}
      onconfigtoggle={onconfigtoggle ? onconfigtoggle : () => (configOpen = true)}
      {onsessionsopen}
      onrename={handleRename}
      ondelete={handleDeleteSession}
      {linkedProject}
      {availableProjects}
      onlink={(projectId) => void handleLinkProject(projectId)}
      onprojectopen={handleProjectChipOpen}
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
    <div
      class="flex flex-1 flex-col items-center justify-center gap-3 px-6 py-8 text-center"
      role="alert"
      aria-live="assertive"
      data-testid="session-corrupted"
    >
      <div
        class="flex h-12 w-12 items-center justify-center rounded-full bg-destructive/15 text-destructive"
        aria-hidden="true"
      >
        <AlertOctagon size={22} />
      </div>
      <div class="max-w-md">
        <p class="text-sm font-medium text-destructive">
          {$t("chat.session_corrupted.title")}
        </p>
        <p class="mt-1 text-xs text-muted-foreground">
          {$t("chat.session_corrupted.description")}
        </p>
        <p class="mt-2 font-mono text-[10px] text-muted-foreground/60">
          {loadError}
        </p>
      </div>
      <div class="flex gap-2">
        <Button
          size="sm"
          variant="outline"
          onclick={exportCorruptedSessionRaw}
          data-testid="session-corrupted-export"
        >
          {$t("chat.session_corrupted.export_raw")}
        </Button>
        <Button
          size="sm"
          variant="destructive"
          onclick={deleteCorruptedSession}
          data-testid="session-corrupted-delete"
        >
          {$t("chat.session_corrupted.delete")}
        </Button>
      </div>
    </div>
  {:else if loadErrorKind === "other"}
    <div
      class="flex flex-1 items-center justify-center px-6"
      role="alert"
      aria-live="polite"
    >
      <p class="text-xs text-destructive">{loadError}</p>
    </div>
  {:else}
    <div class="relative flex-1 min-h-0">
    <div
      bind:this={messagesContainer}
      onscroll={handleScroll}
      class="h-full overflow-y-auto px-4 py-4 space-y-6"
      data-testid="chat-messages-list"
    >
      {#if (messages ?? []).length === 0 && !isStreaming && !isProcessing}
        <div class="flex h-full flex-col items-center justify-center gap-2 text-muted-foreground/40">
          <MessageSquare size={28} />
          <p class="text-xs">{$t("chat.first_message_placeholder")}</p>
        </div>
      {:else}
        <!-- SummarizedMessagesBanner moved to ContextDrawer's Memory tab
             - no longer rendered inline in the message list. -->

        {#each messageGroups as group (group.key)}
          {@const firstMsg = group.messages[0]}
          {@const isSingleCrossSession =
            group.messages.length === 1 &&
            firstMsg.role === "assistant" &&
            Boolean(firstMsg.metadata?.cross_session)}
          <div class="relative" data-message-id={group.key} tabindex="-1">
            <MessageGroup
              {group}
              {sessionId}
              agentName={sessionAgentName}
            />
            {#if $uiMode === "builder" && hasCrossSessionRefs && isSingleCrossSession}
              <div
                class="absolute -top-1 -right-1 flex items-center gap-1 rounded-full bg-secondary/20 px-2 py-0.5"
                data-testid="cross-session-badge"
              >
                <Link size={9} class="text-secondary" />
                <span class="text-[9px] font-medium text-secondary">{$t("chat.past_session")}</span>
              </div>
            {/if}
          </div>
        {/each}

        {#if isStreaming}
          <!-- Streaming bubble is isolated in its own component so only this
               subtree re-renders per token. The committed
               MessageGroup list above depends on the immutable `messages`
               array, so it does NOT re-render on each token. -->
          <StreamingMessage text={tokenBuffer} {sessionMode} />
        {/if}

        {#if activeA2A}
          <div class="flex justify-start" data-testid="chat-a2a-delegating">
            <div class="max-w-full sm:max-w-[85%] overflow-hidden rounded-lg border border-secondary/20 glass-surface px-2.5 py-2">
              <div class="flex items-center gap-1.5">
                <Zap size={11} class="animate-pulse text-secondary" />
                <span class="text-[11px] font-medium text-secondary/80">
                  {$t("chat.a2a_delegating", { values: { agent: activeA2A.target, skill: activeA2A.skill_id } })}
                </span>
                {#if a2aElapsed > 0}
                  <span class="ml-auto flex-shrink-0 text-[10px] text-muted-foreground/40">{a2aElapsed}s</span>
                {/if}
              </div>

              {#if a2aSteps.length > 0}
                <div class="mt-1.5 space-y-0.5">
                  {#each a2aSteps as step, _i (step.step_id)}
                    <div class="flex items-center gap-1.5">
                      <div class="flex-shrink-0">
                        {#if step.status === "running"}
                          <Spinner size={9} class="text-secondary/60" />
                        {:else if step.status === "done"}
                          <Check size={9} class="text-success/70" />
                        {:else}
                          <X size={9} class="text-destructive/70" />
                        {/if}
                      </div>
                      <span class="truncate text-[11px] text-muted-foreground">{step.desc || `step ${step.step_num}`}</span>
                      {#if step.total > 0}
                        <span class="flex-shrink-0 text-[10px] text-muted-foreground/40">{step.step_num}/{step.total}</span>
                      {/if}
                      {#if step.durationMs !== undefined}
                        <span class="ml-auto flex-shrink-0 text-[10px] text-muted-foreground/40">{step.durationMs}ms</span>
                      {/if}
                    </div>
                  {/each}
                </div>
              {/if}

              {#if a2aGuardMessage}
                <div class="mt-1.5 rounded bg-destructive/10 px-2 py-1 text-[10px] text-destructive/80">
                  {a2aGuardMessage}
                </div>
              {/if}
            </div>
          </div>
        {/if}

        {#if liveToolChain.length > 0}
          <div class="flex justify-start" data-testid="chat-live-reasoning">
            <div class="max-w-full sm:max-w-[85%] overflow-hidden rounded-lg border border-border/20 glass-surface px-2.5 py-2">
              <div class="mb-1.5 flex items-center gap-1.5">
                <BrainCircuit size={10} class="text-primary/50" />
                <span class="text-[10px] font-medium text-muted-foreground/60">{$t("chat.reasoning_live")}</span>
              </div>
              <div class="space-y-0.5">
                {#each groupedLiveToolChain as step (step.name)}
                  <div class="flex items-center gap-1.5">
                    <div class="flex-shrink-0">
                      {#if step.status === "running"}
                        <Spinner size={9} class="text-primary/60" />
                      {:else if step.status === "done"}
                        <Check size={9} class="text-success/70" />
                      {:else}
                        <X size={9} class="text-destructive/70" />
                      {/if}
                    </div>
                    <span class="truncate font-mono text-[11px] text-muted-foreground">{step.name}</span>
                    {#if step.count > 1}
                      <span class="flex-shrink-0 text-[10px] text-muted-foreground/50">×{step.count}</span>
                    {/if}
                    {#if step.totalDurationMs > 0}
                      <span class="ml-auto flex-shrink-0 text-[10px] text-muted-foreground/40">{step.totalDurationMs}ms</span>
                    {/if}
                  </div>
                {/each}
              </div>
            </div>
          </div>
        {:else if activeToolName}
          <div class="flex justify-start" data-testid="chat-tool-executing">
            <div class="flex items-center gap-1.5 rounded-lg bg-muted/40 px-3 py-1.5 text-[11px] text-muted-foreground">
              <Spinner size={11} />
              <span>{$t("chat.tool_executing", { values: { tool: activeToolName } })}</span>
            </div>
          </div>
        {/if}

        {#if pendingApproval}
          <div class="flex flex-col items-start gap-1" data-testid="chat-approval-inline">
            <div class="max-w-full sm:max-w-[85%]">
              <ApprovalCard
                sessionId={pendingApproval.sessionId}
                messageId={pendingApproval.messageId}
                toolName={pendingApproval.toolName}
                inputPreview={pendingApproval.inputPreview}
              />
            </div>
            <Button variant="ghost" size="sm"
              type="button"
              class="text-[11px] text-primary hover:underline"
              onclick={() => {
                const pa = pendingApproval;
                if (!pa) return;
                const id = `chat:${pa.sessionId}:${pa.messageId}:${pa.toolName}`;
                if (typeof window !== "undefined") {
                  history.replaceState(null, "", `#inbox?item=${encodeURIComponent(id)}`);
                }
                // Lazy import keeps the chat bundle clean.
                import("$lib/stores/navigation").then((m) => m.navigateTo("inbox"));
              }}
              data-testid="chat-approval-open-inbox"
            >
              {$t("inbox.open_in_inbox")} →
            </Button>
          </div>
        {/if}

        {#if pendingUserInput}
          <div class="flex justify-start" data-testid="chat-ask-user-inline">
            <div class="max-w-full sm:max-w-[85%]">
              <AskUserCard
                requestId={pendingUserInput.requestId}
                questions={pendingUserInput.questions}
                context={pendingUserInput.context}
              />
            </div>
          </div>
        {/if}

        {#if isProcessing && sessionMode === "agent"}
          <div class="flex justify-start" data-testid="chat-agent-loading">
            <div class="flex items-center gap-1.5 rounded-lg bg-muted/40 px-3 py-1.5 text-[11px] text-muted-foreground">
              <Spinner size={11} />
              <span>{$t("chat.agent_processing")}</span>
            </div>
          </div>
        {/if}

        {#if isProcessing && sessionMode === "libre"}
          <div class="flex justify-start">
            <div class="flex items-center gap-1.5 rounded-lg bg-muted/40 px-3 py-1.5 text-[11px] text-muted-foreground">
              <Spinner size={11} />
              <span>{$t("chat.thinking")}</span>
            </div>
          </div>
        {/if}
      {/if}
    </div>
    <ScrollToBottomButton
      visible={showScrollToBottom}
      unreadCount={unreadWhileScrolled}
      onclick={jumpToLatest}
    />
    </div>
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
      onsend={handleSend}
      lastUserMessage={lastUserMessageText}
      oncommand={handleSlashCommand}
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
