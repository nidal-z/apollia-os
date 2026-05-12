<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import { Inbox as InboxIcon, Activity as ActivityIcon, Bell } from "lucide-svelte";

  import {
    pendingApprovals,
    pendingCount,
    requestNotificationPermission,
  } from "$lib/stores/hitl";
  import {
    pendingChatApprovals,
    pendingChatApprovalCount,
    pendingUserInputs,
    pendingUserInputCount,
  } from "$lib/stores/chat";

  import { addToast } from "$lib/components/ui/toast/store";
  import RejectReasonDialog from "../components/inbox/RejectReasonDialog.svelte";
  import AskUserForm from "../components/inbox/AskUserForm.svelte";
  import ActivityRow from "../components/inbox/ActivityRow.svelte";
  import NotificationLog from "../components/notifications/NotificationLog.svelte";
  import { TabBar } from "$lib/components/ui/tabs";
  import { Select } from "$lib/components/ui/select";
  import { navigateTo } from "$lib/stores/navigation";
  import { pendingChatSessionId } from "$lib/stores/chat";

  import {
    PageHeader,
    SectionTitle,
    InboxRow,
    HITLCard,
    EmptyState,
  } from "$lib/components/operator";
  import type { InboxType } from "$lib/components/operator";
  import type { RiskLevel } from "$lib/components/operator";
  import type { AlwaysScope } from "$lib/components/operator/HITLCard.svelte";
  import type {
    AskUserAnswer,
    AskUserQuestion,
    NotificationChannel,
    NotificationLogEntry,
    ResolvedChatApproval,
  } from "$lib/types";
  import { CheckCircle2, XCircle, ShieldCheck } from "lucide-svelte";

  import type { InboxItem, InboxRisk } from "../components/inbox/types";
  import type {
    PendingApproval,
    PendingChatApproval,
    PendingUserInputView,
  } from "$lib/types";

  // ── Tab state ────────────────────────────────────────────────────────────
  type Tab = "pending" | "activity" | "notifications";
  const TAB_STORAGE_KEY = "inbox.active_tab";

  let activeTab = $state<Tab>("pending");

  function loadInitialTab(): Tab {
    if (typeof window === "undefined") return "pending";
    const url = new URL(window.location.href);
    const fromQuery = url.searchParams.get("tab");
    if (fromQuery === "activity" || fromQuery === "notifications" || fromQuery === "pending") {
      return fromQuery;
    }
    const stored = sessionStorage.getItem(TAB_STORAGE_KEY);
    if (stored === "activity" || stored === "notifications" || stored === "pending") {
      return stored;
    }
    return "pending";
  }

  function selectTab(next: string): void {
    if (next !== "pending" && next !== "activity" && next !== "notifications") return;
    activeTab = next;
    try {
      sessionStorage.setItem(TAB_STORAGE_KEY, next);
    } catch {
      /* sessionStorage unavailable — non-fatal */
    }
    if (next === "activity") void loadActivity();
    if (next === "notifications") void loadNotificationLogs();
  }

  // ── State ────────────────────────────────────────────────────────────────
  let loading = $state(true);
  let error = $state<string | null>(null);
  let submitting = $state(false);
  let expandedId = $state<string | null>(null);
  let rejectTarget = $state<InboxItem | null>(null);
  let history = $state<ResolvedChatApproval[]>([]);
  let historyError = $state<string | null>(null);

  // Pending tab — filter chips reduced to "all / approval / ask_user".
  type FilterKey = "all" | "approval" | "ask_user";
  let activeFilter = $state<FilterKey>("all");
  let agentFilter = $state<string>("all");

  // Activity tab state.
  let activityEntries = $state<NotificationLogEntry[]>([]);
  let activityLoading = $state(false);
  let activityError = $state<string | null>(null);
  type ActivityFilter = "all" | "failures" | "degradations" | "llm";
  let activityFilter = $state<ActivityFilter>("all");

  // Notifications tab state (sent log).
  let notificationLogs = $state<NotificationLogEntry[]>([]);
  let channels = $state<NotificationChannel[]>([]);
  let notificationsLoading = $state(false);
  let notificationsError = $state<string | null>(null);

  // ── Risk extraction ──────────────────────────────────────────────────────
  function extractRisk(ctx: Record<string, unknown> | undefined): InboxRisk | undefined {
    if (!ctx || typeof ctx !== "object") return undefined;
    const r = (ctx as { risk?: unknown }).risk;
    if (!r || typeof r !== "object") return undefined;
    const rec = r as Record<string, unknown>;
    const level = rec.level;
    if (level !== "low" && level !== "medium" && level !== "high") return undefined;
    return {
      level,
      summary: typeof rec.summary === "string" ? rec.summary : "",
      impact: typeof rec.impact === "string" ? rec.impact : undefined,
      consequences: Array.isArray(rec.consequences)
        ? (rec.consequences as string[])
        : undefined,
      rationale: typeof rec.rationale === "string" ? rec.rationale : undefined,
      thinking: typeof rec.thinking === "string" ? rec.thinking : undefined,
    };
  }

  // ── Adapters: stores → InboxItem ─────────────────────────────────────────
  function taskToInbox(p: PendingApproval): InboxItem {
    const risk = extractRisk(p.context);
    return {
      id: `task:${p.task_id}`,
      kind: "task",
      agentName: p.agent_name || $t("approvals.unknown_agent"),
      summary: risk?.summary || p.prompt || "-",
      suspendedAt: p.suspended_at,
      risk,
      source: p,
    };
  }

  type ChatKind = "tool" | "filesystem" | "bash" | "always_accept";
  function chatKind(toolName: string): ChatKind {
    if (toolName.startsWith("fs:") || toolName.startsWith("filesystem")) return "filesystem";
    if (toolName.startsWith("bash") || toolName.startsWith("shell")) return "bash";
    return "tool";
  }

  function chatToInbox(c: PendingChatApproval): InboxItem {
    return {
      id: `chat:${c.sessionId}:${c.messageId}:${c.toolName}`,
      kind: chatKind(c.toolName),
      agentName: c.sessionId.slice(0, 8),
      sessionId: c.sessionId,
      toolName: c.toolName,
      summary: c.inputPreview.slice(0, 140),
      suspendedAt: c.receivedAt,
      source: c,
    };
  }

  function askUserToInbox(u: PendingUserInputView): InboxItem {
    let parsed: unknown[] = [];
    try {
      const raw = JSON.parse(u.questions_json);
      if (Array.isArray(raw)) parsed = raw;
    } catch {
      /* ignore — empty list keeps the form harmless */
    }
    const firstQ =
      (parsed[0] as { question?: string } | undefined)?.question ?? "Question";
    return {
      id: `ask_user:${u.request_id}`,
      kind: "ask_user" as const,
      agentName: u.session_id ? u.session_id.slice(0, 8) : "agent",
      sessionId: u.session_id || undefined,
      summary: firstQ.slice(0, 140),
      suspendedAt: u.created_at,
      source: u,
      questions: parsed,
    };
  }

  /**
   * Normalize the raw `questions_json` payload coming from the runtime into
   * the strongly-typed shape expected by `<AskUserForm>`.
   *
   * The runtime uses snake_case `question_type`; the form uses `type`. We
   * coerce here so the form stays free of payload-shape concerns.
   */
  function questionsForForm(raw: unknown[]): AskUserQuestion[] {
    const out: AskUserQuestion[] = [];
    for (const entry of raw) {
      if (!entry || typeof entry !== "object") continue;
      const rec = entry as Record<string, unknown>;
      const id = typeof rec.id === "string" ? rec.id : undefined;
      const question = typeof rec.question === "string" ? rec.question : undefined;
      if (!id || !question) continue;
      const rawType =
        (typeof rec.type === "string" ? rec.type : undefined) ??
        (typeof rec.question_type === "string" ? rec.question_type : undefined);
      const type: AskUserQuestion["type"] =
        rawType === "single_choice" || rawType === "multi_choice" ? rawType : "open";
      const options = Array.isArray(rec.options)
        ? (rec.options as unknown[]).filter((o): o is string => typeof o === "string")
        : [];
      const hint = typeof rec.hint === "string" ? rec.hint : undefined;
      out.push({ id, question, type, options, hint });
    }
    return out;
  }

  function contextForForm(item: InboxItem): string | undefined {
    if (item.kind !== "ask_user") return undefined;
    const ctx = (item.source as PendingUserInputView).context;
    return typeof ctx === "string" && ctx.trim().length > 0 ? ctx : undefined;
  }

  // ── Derived inbox stream ─────────────────────────────────────────────────
  const allItems = $derived.by<InboxItem[]>(() => {
    const task = $pendingApprovals.map(taskToInbox);
    const chat = $pendingChatApprovals.map(chatToInbox);
    const askUser = $pendingUserInputs.map(askUserToInbox);
    return [...task, ...chat, ...askUser].sort(
      (a, b) =>
        new Date(b.suspendedAt).getTime() - new Date(a.suspendedAt).getTime(),
    );
  });

  const totalPending = $derived(
    $pendingCount + $pendingChatApprovalCount + $pendingUserInputCount,
  );

  function rowType(item: InboxItem): InboxType {
    return item.kind === "ask_user" ? "question" : "approval";
  }

  function rowFilterKey(item: InboxItem): FilterKey {
    return item.kind === "ask_user" ? "ask_user" : "approval";
  }

  // Distinct agents for the agent <Select>.
  const agentOptions = $derived.by(() => {
    const set = new Map<string, string>();
    for (const i of allItems) set.set(i.agentName, i.agentName);
    return [...set.values()].sort((a, b) => a.localeCompare(b));
  });

  const filteredItems = $derived.by(() => {
    let list = allItems;
    if (activeFilter !== "all") {
      list = list.filter((i) => rowFilterKey(i) === activeFilter);
    }
    if (agentFilter !== "all") {
      list = list.filter((i) => i.agentName === agentFilter);
    }
    return list;
  });

  // Counts per filter (for chip labels). Always derived from the agent-scoped
  // list so the chip numbers reflect what the user is currently looking at.
  const counts = $derived.by(() => {
    const scoped =
      agentFilter === "all"
        ? allItems
        : allItems.filter((i) => i.agentName === agentFilter);
    const c: Record<FilterKey, number> = { all: scoped.length, approval: 0, ask_user: 0 };
    for (const i of scoped) {
      c[rowFilterKey(i)] += 1;
    }
    return c;
  });

  // ── Date grouping ────────────────────────────────────────────────────────
  type GroupKey = "today" | "yesterday" | "earlier";

  function groupOf(iso: string): GroupKey {
    const d = new Date(iso);
    const now = new Date();
    const startToday = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();
    const startYesterday = startToday - 24 * 60 * 60 * 1000;
    const ts = d.getTime();
    if (ts >= startToday) return "today";
    if (ts >= startYesterday) return "yesterday";
    return "earlier";
  }

  const grouped = $derived.by(() => {
    const g: Record<GroupKey, InboxItem[]> = {
      today: [],
      yesterday: [],
      earlier: [],
    };
    for (const i of filteredItems) {
      g[groupOf(i.suspendedAt)].push(i);
    }
    return g;
  });

  const GROUP_ORDER: GroupKey[] = ["today", "yesterday", "earlier"];

  // ── Time formatting ──────────────────────────────────────────────────────
  function relTime(iso: string): string {
    const ms = Date.now() - new Date(iso).getTime();
    const min = Math.floor(ms / 60000);
    if (min < 1) return "à l'instant";
    if (min < 60) return `il y a ${min} min`;
    const h = Math.floor(min / 60);
    if (h < 24) return `il y a ${h} h`;
    const days = Math.floor(h / 24);
    if (days === 1) return "hier";
    return `il y a ${days} j`;
  }

  // ── HITL inline card data ────────────────────────────────────────────────
  function riskLevel(item: InboxItem): RiskLevel {
    if (item.kind === "task" && item.risk) {
      if (item.risk.level === "medium") return "medium";
      if (item.risk.level === "high") return "high";
      return "low";
    }
    if (item.kind === "bash" || item.kind === "filesystem") return "medium";
    return "low";
  }

  function actionLabel(item: InboxItem): string {
    return item.summary || $t("inbox.title_operator");
  }

  function expiresLabel(item: InboxItem): string | undefined {
    void item;
    return undefined;
  }

  // ── Lifecycle ────────────────────────────────────────────────────────────
  onMount(() => {
    activeTab = loadInitialTab();
    requestNotificationPermission();
    void loadData();
    // Allow other routes (Notifications page) to deep-link a specific tab.
    function onSelectTab(ev: Event) {
      const detail = (ev as CustomEvent).detail as { tab?: string } | undefined;
      if (detail?.tab) selectTab(detail.tab);
    }
    window.addEventListener("apollia:inbox:select-tab", onSelectTab);
    if (activeTab === "activity") void loadActivity();
    if (activeTab === "notifications") void loadNotificationLogs();
    return () => {
      window.removeEventListener("apollia:inbox:select-tab", onSelectTab);
    };
  });

  async function loadData(): Promise<void> {
    loading = true;
    error = null;
    try {
      const [pending, userInputs] = await Promise.all([
        invoke<PendingApproval[]>("list_pending_approvals"),
        invoke<PendingUserInputView[]>("list_pending_user_inputs").catch(() => []),
      ]);
      pendingApprovals.set(pending);
      for (const u of userInputs) {
        const { addPendingUserInput: add } = await import("$lib/stores/chat");
        add(u);
      }
    } catch (err: unknown) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      loading = false;
    }
    await loadHistory();
  }

  async function loadHistory(): Promise<void> {
    historyError = null;
    try {
      history = await invoke<ResolvedChatApproval[]>("list_chat_approval_history", {
        limit: 50,
        days: 14,
      });
    } catch (err: unknown) {
      historyError = err instanceof Error ? err.message : String(err);
      history = [];
    }
  }

  async function loadActivity(): Promise<void> {
    activityLoading = true;
    activityError = null;
    try {
      activityEntries = await invoke<NotificationLogEntry[]>("list_runtime_activity", {
        days: 14,
      });
    } catch (err: unknown) {
      activityError = err instanceof Error ? err.message : String(err);
      activityEntries = [];
    } finally {
      activityLoading = false;
    }
  }

  async function loadNotificationLogs(): Promise<void> {
    notificationsLoading = true;
    notificationsError = null;
    try {
      const [logs, ch] = await Promise.all([
        invoke<NotificationLogEntry[]>("get_notification_logs", { limit: 50 }),
        invoke<NotificationChannel[]>("list_notification_channels").catch(() => []),
      ]);
      notificationLogs = logs;
      channels = ch;
    } catch (err: unknown) {
      notificationsError = err instanceof Error ? err.message : String(err);
      notificationLogs = [];
    } finally {
      notificationsLoading = false;
    }
  }

  // ── Resolve handlers (preserved invoke calls) ────────────────────────────
  async function resolveItem(
    item: InboxItem,
    approved: boolean,
    reason?: string,
  ): Promise<void> {
    if (item.kind === "task") {
      await invoke("resume_task", {
        taskId: item.source.task_id,
        approved,
        reason: reason ?? null,
      });
    } else if (item.kind === "ask_user") {
      // ask_user resolution is handled through `respondAskUser` /
      // `rejectAskUser` so the structured answers flow back to the agent.
      // This branch only runs for the unlikely outer "Approve / Reject"
      // path — never invoked from the redesigned form.
      if (!approved && reason) {
        await invoke("respond_user_input_rejected", {
          requestId: item.source.request_id,
          reason,
        });
      }
    } else {
      await invoke("authorize_chat_tool", {
        sessionId: item.source.sessionId,
        messageId: item.source.messageId,
        toolName: item.source.toolName,
        decision: approved ? "accept" : "refuse",
        reason: reason ?? null,
      });
    }
  }

  async function resolveAlwaysAccept(item: InboxItem, scope: AlwaysScope): Promise<void> {
    if (item.kind === "task" || item.kind === "ask_user") return;
    await invoke("authorize_chat_tool", {
      sessionId: item.source.sessionId,
      messageId: item.source.messageId,
      toolName: item.source.toolName,
      decision: "always_accept",
      scope,
    });
  }

  function isChatToolItem(item: InboxItem): boolean {
    return item.kind === "tool" || item.kind === "filesystem" || item.kind === "bash";
  }

  async function handleAlwaysAccept(item: InboxItem, scope: AlwaysScope): Promise<void> {
    submitting = true;
    try {
      await resolveAlwaysAccept(item, scope);
      addToast($t("inbox.toast.always_accepted"), "success");
      if (expandedId === item.id) expandedId = null;
      await loadHistory();
    } catch (err: unknown) {
      addToast(err instanceof Error ? err.message : String(err), "error");
    } finally {
      submitting = false;
    }
  }

  async function handleApprove(item: InboxItem): Promise<void> {
    submitting = true;
    try {
      await resolveItem(item, true);
      addToast($t("inbox.toast.accepted"), "success");
      if (expandedId === item.id) expandedId = null;
      await loadHistory();
    } catch (err: unknown) {
      addToast(err instanceof Error ? err.message : String(err), "error");
    } finally {
      submitting = false;
    }
  }

  function openReject(item: InboxItem): void {
    rejectTarget = item;
  }

  async function confirmReject(reason: string): Promise<void> {
    if (!rejectTarget) return;
    submitting = true;
    try {
      await resolveItem(rejectTarget, false, reason);
      addToast($t("inbox.toast.rejected"), "success");
      if (expandedId === rejectTarget.id) expandedId = null;
      rejectTarget = null;
      await loadHistory();
    } catch (err: unknown) {
      addToast(err instanceof Error ? err.message : String(err), "error");
    } finally {
      submitting = false;
    }
  }

  function toggleExpand(item: InboxItem): void {
    expandedId = expandedId === item.id ? null : item.id;
  }

  // ── ask_user — structured submit ─────────────────────────────────────────
  async function respondAskUser(item: InboxItem, answers: AskUserAnswer[]): Promise<void> {
    if (item.kind !== "ask_user") return;
    submitting = true;
    try {
      await invoke("respond_user_input", {
        requestId: item.source.request_id,
        answers,
      });
      addToast($t("inbox.toast.ask_user_replied"), "success");
      if (expandedId === item.id) expandedId = null;
    } catch (err: unknown) {
      addToast(err instanceof Error ? err.message : String(err), "error");
    } finally {
      submitting = false;
    }
  }

  /** Jump to the source chat session for an ask_user item.
   *
   *  Sets the `pendingChatSessionId` store BEFORE navigation so Chat.svelte's
   *  `onMount` subscription picks the right session as soon as the route
   *  mounts. The previous CustomEvent-based path was a no-op — nothing
   *  in the codebase actually listened for `apollia:chat:open-session`. */
  function openAskUserChat(sessionId: string): void {
    pendingChatSessionId.set(sessionId);
    navigateTo("chat");
  }

  async function rejectAskUser(item: InboxItem, reason: string): Promise<void> {
    if (item.kind !== "ask_user") return;
    submitting = true;
    try {
      await invoke("respond_user_input_rejected", {
        requestId: item.source.request_id,
        reason,
      });
      addToast($t("inbox.toast.rejected"), "success");
      if (expandedId === item.id) expandedId = null;
    } catch (err: unknown) {
      addToast(err instanceof Error ? err.message : String(err), "error");
    } finally {
      submitting = false;
    }
  }

  // ── Filter chip definitions ──────────────────────────────────────────────
  const ACTIVITY_GROUPS: Record<ActivityFilter, string[]> = {
    all: [],
    failures: ["task.failed", "trigger.error"],
    degradations: ["agent.degraded"],
    llm: ["llm.backend_down"],
  };

  const filteredActivity = $derived.by(() => {
    if (activityFilter === "all") return activityEntries;
    const wanted = new Set(ACTIVITY_GROUPS[activityFilter]);
    return activityEntries.filter((e) => wanted.has(e.event_name));
  });

  // Tab items + counts
  const tabItems = $derived([
    {
      key: "pending",
      label: $t("inbox.tab_pending"),
      count: totalPending,
    },
    {
      key: "activity",
      label: $t("inbox.tab_activity"),
      count: activityEntries.length,
    },
    {
      key: "notifications",
      label: $t("inbox.tab_notifications"),
      count: notificationLogs.length,
    },
  ]);
</script>

<div class="flex h-full min-h-0 w-full flex-col overflow-hidden" data-testid="inbox-page">
  <PageHeader
    kicker={$t("inbox.kicker")}
    title={$t("inbox.title_operator")}
    subtitle={activeTab === "pending" && totalPending > 0
      ? $t("inbox.pending_count", { values: { count: totalPending } })
      : $t("inbox.subtitle")}
  />

  <!-- Tab bar -------------------------------------------------------- -->
  <div class="px-8 pt-3">
    <TabBar
      items={tabItems}
      activeTab={activeTab}
      ontabchange={selectTab}
      testidPrefix="inbox"
    />
  </div>

  {#if activeTab === "pending"}
    <!-- Filter chips + agent select -------------------------------- -->
    <div class="flex flex-wrap items-center justify-between gap-2 px-8 pt-3 pb-2">
      <div class="flex flex-wrap items-center gap-1.5">
        {#each [{ key: "all", labelKey: "inbox.chips.all" }, { key: "approval", labelKey: "inbox.chips.approvals" }, { key: "ask_user", labelKey: "inbox.chips.questions" }] as f (f.key)}
          {@const isActive = activeFilter === f.key}
          <button
            type="button"
            class="rounded-full text-[11px] font-medium transition-colors px-2.5 py-1 border {isActive
              ? 'bg-primary/10 text-primary border-primary/20'
              : 'bg-transparent text-muted-foreground border-border hover:bg-muted/40'}"
            onclick={() => (activeFilter = f.key as FilterKey)}
            aria-pressed={isActive}
            data-testid="inbox-filter-{f.key}"
          >
            {$t(f.labelKey)} · {counts[f.key as FilterKey]}
          </button>
        {/each}
      </div>

      {#if agentOptions.length > 0}
        <div class="flex items-center gap-2">
          <label for="inbox-agent-filter" class="text-[11px] text-muted-foreground">
            {$t("inbox.filter_by_agent")}
          </label>
          <Select
            id="inbox-agent-filter"
            class="h-8 w-auto text-xs"
            bind:value={agentFilter}
            data-testid="inbox-agent-filter"
          >
            <option value="all">{$t("inbox.all_agents")}</option>
            {#each agentOptions as agent}
              <option value={agent}>{agent}</option>
            {/each}
          </Select>
        </div>
      {/if}
    </div>

    <!-- Pending list ----------------------------------------------- -->
    <div class="flex-1 min-h-0 overflow-y-auto">
      {#if loading}
        <p class="px-8 py-6 text-sm text-muted-foreground">{$t("common.loading")}</p>
      {:else if error}
        <p class="px-8 py-6 text-sm text-destructive">{error}</p>
      {:else if filteredItems.length === 0}
        <div class="px-8 py-10">
          <EmptyState
            title={$t("inbox.empty_title")}
            desc={$t("inbox.empty_subtitle")}
            tone="success"
          >
            {#snippet icon()}<InboxIcon size={22} />{/snippet}
          </EmptyState>
        </div>
      {:else}
        {#each GROUP_ORDER as g (g)}
          {#if grouped[g].length > 0}
            <SectionTitle count={grouped[g].length}>{$t(`inbox.group.${g}`)}</SectionTitle>
            <div class="px-8 pb-2">
              <div class="rounded-xl border border-border overflow-hidden bg-card">
                {#each grouped[g] as item (item.id)}
                  {@const isApproval = item.kind === "task" || item.kind === "tool" || item.kind === "filesystem" || item.kind === "bash" || item.kind === "ask_user"}
                  {@const isExpanded = expandedId === item.id}
                  <div>
                    <InboxRow
                      type={rowType(item)}
                      title={item.summary || "—"}
                      agent={item.agentName}
                      timestamp={relTime(item.suspendedAt)}
                      unread={true}
                      onclick={isApproval ? () => toggleExpand(item) : undefined}
                      onSecondaryAction={item.kind === "ask_user" && item.sessionId
                        ? () => openAskUserChat(item.sessionId!)
                        : undefined}
                      secondaryLabel={item.kind === "ask_user"
                        ? $t("inbox.ask_user.open_chat")
                        : undefined}
                    />
                    {#if isExpanded && isApproval}
                      <div class="px-4 py-3 border-b border-border bg-muted/20">
                        {#if item.kind === "ask_user"}
                          <AskUserForm
                            questions={questionsForForm(item.questions)}
                            context={contextForForm(item)}
                            sessionId={item.sessionId}
                            {submitting}
                            onsubmit={(answers) => respondAskUser(item, answers)}
                            oncancel={(reason) => rejectAskUser(item, reason)}
                            onOpenChat={item.sessionId
                              ? () => openAskUserChat(item.sessionId!)
                              : undefined}
                          />
                        {:else}
                          <HITLCard
                            agent={item.agentName}
                            action={actionLabel(item)}
                            risk={riskLevel(item)}
                            tool={item.toolName}
                            scope={item.kind === "task" ? item.risk?.impact : undefined}
                            summary={item.risk?.rationale}
                            params={item.risk?.consequences ?? []}
                            expires={expiresLabel(item)}
                            onApprove={() => handleApprove(item)}
                            onReject={() => openReject(item)}
                            onAlwaysAccept={isChatToolItem(item)
                              ? (s) => handleAlwaysAccept(item, s)
                              : undefined}
                            hasProject={false}
                          />
                        {/if}
                      </div>
                    {/if}
                  </div>
                {/each}
              </div>
            </div>
          {/if}
        {/each}
      {/if}

      <!-- Historique récent (HITL résolu) -------------------------- -->
      {#if !loading && (history.length > 0 || historyError)}
        <SectionTitle count={history.length}>{$t("inbox.history_title")}</SectionTitle>
        <div class="px-8 pb-10">
          {#if historyError}
            <p class="text-xs text-destructive">{historyError}</p>
          {:else}
            <ul class="rounded-xl border border-border bg-card divide-y divide-border/60">
              {#each history as h (h.message_id + "::" + h.tool_name + "::" + h.resolved_at)}
                {@const isAccept = h.decision === "accept"}
                {@const isAlways = h.decision === "always_accept"}
                {@const isRefuse = h.decision === "refuse"}
                <li class="flex items-start gap-3 px-4 py-2.5 text-[12px]">
                  <span class="shrink-0 mt-0.5">
                    {#if isAccept}
                      <CheckCircle2 size={14} class="text-success" />
                    {:else if isAlways}
                      <ShieldCheck size={14} class="text-primary" />
                    {:else if isRefuse}
                      <XCircle size={14} class="text-destructive" />
                    {/if}
                  </span>
                  <div class="flex-1 min-w-0">
                    <div class="flex items-baseline justify-between gap-2">
                      <div class="min-w-0 flex items-baseline gap-2">
                        <code class="font-mono text-[11.5px] text-foreground truncate">{h.tool_name}</code>
                        <span class="text-[10.5px] text-muted-foreground">
                          {#if isAccept}{$t("inbox.history.accepted")}{:else if isAlways}{$t("inbox.history.always_accepted")}{:else}{$t("inbox.history.refused")}{/if}
                        </span>
                      </div>
                      <span class="text-[10.5px] text-muted-foreground/70 font-mono shrink-0" title={h.resolved_at}>
                        {relTime(h.resolved_at)}
                      </span>
                    </div>
                    {#if isRefuse && h.reason}
                      <p class="mt-0.5 text-[11px] text-destructive/80 line-clamp-2" title={h.reason}>
                        <span class="font-medium">{$t("inbox.history.reason")}</span> {h.reason}
                      </p>
                    {/if}
                    <p class="mt-0.5 text-[10.5px] text-muted-foreground/60">
                      {$t("inbox.history.session")} <code class="font-mono">{h.session_id.slice(0, 8)}</code>
                    </p>
                  </div>
                </li>
              {/each}
            </ul>
          {/if}
        </div>
      {/if}
    </div>
  {:else if activeTab === "activity"}
    <!-- Activity filter chips -------------------------------------- -->
    <div class="flex flex-wrap items-center gap-1.5 px-8 pt-3 pb-2">
      {#each [{ key: "all", labelKey: "inbox.activity.filter.all" }, { key: "failures", labelKey: "inbox.activity.filter.failures" }, { key: "degradations", labelKey: "inbox.activity.filter.degradations" }, { key: "llm", labelKey: "inbox.activity.filter.llm" }] as f (f.key)}
        {@const isActive = activityFilter === f.key}
        <button
          type="button"
          class="rounded-full text-[11px] font-medium transition-colors px-2.5 py-1 border {isActive
            ? 'bg-primary/10 text-primary border-primary/20'
            : 'bg-transparent text-muted-foreground border-border hover:bg-muted/40'}"
          onclick={() => (activityFilter = f.key as ActivityFilter)}
          aria-pressed={isActive}
          data-testid="inbox-activity-filter-{f.key}"
        >
          {$t(f.labelKey)}
        </button>
      {/each}
    </div>

    <div class="flex-1 min-h-0 overflow-y-auto px-8 pb-10">
      {#if activityLoading}
        <p class="py-6 text-sm text-muted-foreground">{$t("common.loading")}</p>
      {:else if activityError}
        <p class="py-6 text-sm text-destructive">{activityError}</p>
      {:else if filteredActivity.length === 0}
        <div class="py-10">
          <EmptyState
            title={$t("inbox.activity.empty")}
            desc={$t("inbox.activity.empty_desc")}
            tone="success"
          >
            {#snippet icon()}<ActivityIcon size={22} />{/snippet}
            {#snippet action()}
              <button
                type="button"
                class="text-[12px] text-primary underline-offset-2 hover:underline"
                onclick={() => navigateTo("observability")}
                data-testid="inbox-activity-empty-cta"
              >
                {$t("inbox.activity.empty_cta")}
              </button>
            {/snippet}
          </EmptyState>
        </div>
      {:else}
        <div class="flex flex-col gap-2 pt-3">
          {#each filteredActivity as entry (entry.id)}
            <ActivityRow {entry} />
          {/each}
        </div>
      {/if}
    </div>
  {:else}
    <!-- Notifications envoyées tab -------------------------------- -->
    <div class="flex-1 min-h-0 overflow-y-auto px-8 pb-10 pt-3">
      {#if notificationsLoading}
        <p class="py-6 text-sm text-muted-foreground">{$t("common.loading")}</p>
      {:else if notificationsError}
        <p class="py-6 text-sm text-destructive">{notificationsError}</p>
      {:else if notificationLogs.length === 0}
        <div class="py-10">
          <EmptyState
            title={$t("inbox.notifications.empty")}
            desc={$t("inbox.notifications.empty_desc")}
            tone="neutral"
          >
            {#snippet icon()}<Bell size={22} />{/snippet}
          </EmptyState>
        </div>
      {:else}
        <NotificationLog logs={notificationLogs} {channels} />
      {/if}
    </div>
  {/if}

  <RejectReasonDialog
    open={rejectTarget !== null}
    {submitting}
    title={$t("inbox.reject_title")}
    onclose={() => (rejectTarget = null)}
    onconfirm={confirmReject}
  />
</div>
