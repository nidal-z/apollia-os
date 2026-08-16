<script lang="ts">
  import { onMount } from "svelte";
  import { t, locale } from "svelte-i18n";
  import { formatRelativeTime } from "$lib/utils";

  import { pendingApprovals, pendingCount, requestNotificationPermission } from "$lib/stores/hitl";
  import {
    pendingChatApprovals,
    pendingChatApprovalCount,
    pendingUserInputs,
    pendingUserInputCount,
    pendingChatSessionId,
  } from "$lib/stores/chat";
  import { pendingPlanApprovals, pendingPlanApprovalCount } from "$lib/stores/chat-global";
  import { addToast } from "$lib/components/ui/toast/store";
  import { reportError } from "$lib/errors/reportError";
  import { navigateTo } from "$lib/stores/navigation";

  import { TabBar } from "$lib/components/ui/tabs";
  import RouteTransition from "$lib/components/ui/route-transition/RouteTransition.svelte";
  import { PageHeader } from "$lib/components/operator";
  import type { AlwaysScope } from "$lib/components/operator/HITLCard.svelte";

  import RejectReasonDialog from "../components/inbox/RejectReasonDialog.svelte";
  import InboxPendingTab from "../components/inbox/InboxPendingTab.svelte";
  import InboxActivityTab from "../components/inbox/InboxActivityTab.svelte";
  import InboxNotificationsTab from "../components/inbox/InboxNotificationsTab.svelte";

  import {
    listPendingApprovals,
    listPendingUserInputs,
    listChatApprovalHistory,
    listResolvedApprovals,
    listRuntimeActivity,
    getNotificationLogs,
    listNotificationChannels,
    respondUserInput,
    respondUserInputRejected,
  } from "$lib/ipc/inbox";
  import {
    taskToInbox,
    chatToInbox,
    planToInbox,
    askUserToInbox,
    type ActivityFilter,
    type Translate,
  } from "../components/inbox/inboxModel";
  import { resolveItem, alwaysAccept } from "../components/inbox/inboxActions";
  import { mergeApprovalHistory, type HistoryEntry } from "../components/inbox/historyModel";
  import { itemId } from "../components/inbox/rowMenu";
  import type { InboxItem } from "../components/inbox/types";
  import type {
    AskUserAnswer,
    NotificationChannel,
    NotificationLogEntry,
  } from "$lib/types";

  type Tab = "pending" | "activity" | "notifications";
  const TAB_STORAGE_KEY = "inbox.active_tab";
  // Same window for both history origins so the merged list has one meaning.
  const HISTORY_LIMIT = 50;
  const HISTORY_DAYS = 14;

  let activeTab = $state<Tab>("pending");
  let loading = $state(true);
  let error = $state<unknown>(null);
  let userInputsError = $state<unknown>(null);
  let submitting = $state(false);
  let expandedId = $state<string | null>(null);
  let rejectTarget = $state<InboxItem | null>(null);
  let history = $state<HistoryEntry[]>([]);
  let historyError = $state<unknown>(null);
  let historyPartialError = $state<unknown>(null);
  let activityEntries = $state<NotificationLogEntry[]>([]);
  let activityLoading = $state(false);
  let activityError = $state<unknown>(null);
  let activityFilter = $state<ActivityFilter>("all");
  let notificationLogs = $state<NotificationLogEntry[]>([]);
  let channels = $state<NotificationChannel[]>([]);
  let notificationsLoading = $state(false);
  let notificationsError = $state<unknown>(null);
  let channelsError = $state<unknown>(null);

  const relTime = (iso: string) => formatRelativeTime(iso, $locale ?? "en");

  const allItems = $derived.by<InboxItem[]>(() => {
    const tr = $t as Translate;
    const items = [
      ...$pendingApprovals.map((p) => taskToInbox(p, tr)),
      ...$pendingChatApprovals.map(chatToInbox),
      ...$pendingUserInputs.map((u) => askUserToInbox(u, tr)),
      ...$pendingPlanApprovals.map((p) => planToInbox(p, tr)),
    ];
    return items.sort(
      (a, b) => new Date(b.suspendedAt).getTime() - new Date(a.suspendedAt).getTime(),
    );
  });

  const totalPending = $derived(
    $pendingCount + $pendingChatApprovalCount + $pendingUserInputCount + $pendingPlanApprovalCount,
  );

  const tabItems = $derived([
    { key: "pending", label: $t("inbox.tab_pending"), count: totalPending },
    { key: "activity", label: $t("inbox.tab_activity"), count: activityEntries.length },
    { key: "notifications", label: $t("inbox.tab_notifications"), count: notificationLogs.length },
  ]);

  function loadInitialTab(): Tab {
    if (typeof window === "undefined") return "pending";
    const fromQuery = new URL(window.location.href).searchParams.get("tab");
    if (fromQuery === "activity" || fromQuery === "notifications" || fromQuery === "pending") return fromQuery;
    const stored = sessionStorage.getItem(TAB_STORAGE_KEY);
    if (stored === "activity" || stored === "notifications" || stored === "pending") return stored;
    return "pending";
  }

  function selectTab(next: string): void {
    if (next !== "pending" && next !== "activity" && next !== "notifications") return;
    activeTab = next;
    try {
      sessionStorage.setItem(TAB_STORAGE_KEY, next);
    } catch {
      // REASON: sessionStorage can be unavailable (private mode / quota); the
      // active tab is non-critical UI state, so failing to persist it is safe
      // to ignore. This is not a data path - nothing is silently lost.
    }
    if (next === "activity") void loadActivity();
    if (next === "notifications") void loadNotificationLogs();
  }

  function toggleExpand(item: InboxItem): void {
    expandedId = expandedId === item.id ? null : item.id;
  }

  /**
   * Runs a resolving action with the shared busy / success-toast / humanized-
   * error envelope. A resolved item leaves the store and its row unmounts, so a
   * stale `expandedId` matches nothing and needs no explicit reset.
   */
  async function runResolve(
    action: () => Promise<void>,
    toastKey: string,
    reloadHistory = true,
  ): Promise<void> {
    submitting = true;
    try {
      await action();
      addToast($t(toastKey), "success");
      if (reloadHistory) await loadHistory();
    } catch (e) {
      reportError(e, { surface: "toast" });
    } finally {
      submitting = false;
    }
  }

  const handleApprove = (item: InboxItem) =>
    runResolve(() => resolveItem(item, true), "inbox.toast.accepted");

  const handleAlwaysAccept = (item: InboxItem, scope: AlwaysScope) =>
    runResolve(() => alwaysAccept(item, scope), "inbox.toast.always_accepted");

  function openReject(item: InboxItem): void {
    rejectTarget = item;
  }

  async function confirmReject(reason: string): Promise<void> {
    if (!rejectTarget) return;
    const target = rejectTarget;
    await runResolve(() => resolveItem(target, false, reason), "inbox.toast.rejected");
    rejectTarget = null;
  }

  function respondAskUser(item: InboxItem, answers: AskUserAnswer[]): void {
    if (item.kind !== "ask_user") return;
    const requestId = item.source.request_id;
    void runResolve(() => respondUserInput(requestId, answers), "inbox.toast.ask_user_replied", false);
  }

  function rejectAskUser(item: InboxItem, reason: string): void {
    if (item.kind !== "ask_user") return;
    const requestId = item.source.request_id;
    void runResolve(() => respondUserInputRejected(requestId, reason), "inbox.toast.rejected", false);
  }

  function openAskUserChat(sessionId: string): void {
    pendingChatSessionId.set(sessionId);
    navigateTo("chat");
  }

  async function copyItemId(item: InboxItem): Promise<void> {
    try {
      await navigator.clipboard.writeText(itemId(item));
      addToast($t("inbox.toast.copied"), "success");
    } catch (e) {
      reportError(e, { surface: "toast" });
    }
  }

  async function loadData(): Promise<void> {
    loading = true;
    error = null;
    userInputsError = null;
    const [pendingRes, inputsRes] = await Promise.allSettled([
      listPendingApprovals(),
      listPendingUserInputs(),
    ]);
    if (pendingRes.status === "fulfilled") pendingApprovals.set(pendingRes.value);
    else error = pendingRes.reason;
    if (inputsRes.status === "fulfilled") {
      const { addPendingUserInput } = await import("$lib/stores/chat");
      for (const u of inputsRes.value) addPendingUserInput(u);
    } else {
      userInputsError = inputsRes.reason;
    }
    loading = false;
    await loadHistory();
  }

  /**
   * Loads both origins of the decision history (chat authorizations and agent
   * task approvals) and merges them into one date-sorted list. One failing
   * origin degrades to a partial error above the surviving rows; only a double
   * failure replaces the section with the error box.
   */
  async function loadHistory(): Promise<void> {
    const [chatRes, taskRes] = await Promise.allSettled([
      listChatApprovalHistory(HISTORY_LIMIT, HISTORY_DAYS),
      listResolvedApprovals(HISTORY_LIMIT, HISTORY_DAYS),
    ]);
    const failures: unknown[] = [];
    if (chatRes.status === "rejected") failures.push(chatRes.reason);
    if (taskRes.status === "rejected") failures.push(taskRes.reason);

    if (failures.length === 2) {
      history = [];
      historyError = failures[0];
      historyPartialError = null;
      return;
    }

    history = mergeApprovalHistory(
      chatRes.status === "fulfilled" ? chatRes.value : [],
      taskRes.status === "fulfilled" ? taskRes.value : [],
    );
    historyError = null;
    historyPartialError = failures.length === 1 ? failures[0] : null;
  }

  async function loadActivity(): Promise<void> {
    activityLoading = true;
    try {
      activityEntries = await listRuntimeActivity(14);
      activityError = null;
    } catch (e) {
      activityError = e;
      activityEntries = [];
    } finally {
      activityLoading = false;
    }
  }

  async function loadNotificationLogs(): Promise<void> {
    notificationsLoading = true;
    const [logsRes, chRes] = await Promise.allSettled([
      getNotificationLogs(50),
      listNotificationChannels(),
    ]);
    if (logsRes.status === "fulfilled") {
      notificationLogs = logsRes.value;
      notificationsError = null;
    } else {
      notificationsError = logsRes.reason;
      notificationLogs = [];
    }
    if (chRes.status === "fulfilled") {
      channels = chRes.value;
      channelsError = null;
    } else {
      channelsError = chRes.reason;
      channels = [];
    }
    notificationsLoading = false;
  }

  onMount(() => {
    activeTab = loadInitialTab();
    requestNotificationPermission();
    void loadData();
    void loadActivity();
    void loadNotificationLogs();
    function onSelectTab(ev: Event) {
      const detail = (ev as CustomEvent).detail as { tab?: string } | undefined;
      if (detail?.tab) selectTab(detail.tab);
    }
    window.addEventListener("apollia:inbox:select-tab", onSelectTab);
    return () => window.removeEventListener("apollia:inbox:select-tab", onSelectTab);
  });
</script>

<RouteTransition>
  <div class="flex h-full min-h-0 w-full flex-col overflow-hidden" data-testid="inbox-page">
    <PageHeader
      title={$t("inbox.title_operator")}
      subtitle={activeTab === "pending" && totalPending > 0
        ? $t("inbox.pending_count", { values: { count: totalPending } })
        : $t("inbox.subtitle")}
    />

    <div class="px-8 pt-3">
      <TabBar items={tabItems} {activeTab} ontabchange={selectTab} testidPrefix="inbox" />
    </div>

    {#if activeTab === "pending"}
      <InboxPendingTab
        items={allItems}
        {loading}
        {error}
        partialError={userInputsError}
        {history}
        {historyError}
        {historyPartialError}
        {submitting}
        {expandedId}
        {relTime}
        onToggleExpand={toggleExpand}
        onApprove={handleApprove}
        onReject={openReject}
        onAlwaysAccept={handleAlwaysAccept}
        onRespondAskUser={respondAskUser}
        onRejectAskUser={rejectAskUser}
        onOpenChat={openAskUserChat}
        onCopyId={copyItemId}
        onRetry={loadData}
        onRetryHistory={loadHistory}
      />
    {:else if activeTab === "activity"}
      <InboxActivityTab
        entries={activityEntries}
        loading={activityLoading}
        error={activityError}
        filter={activityFilter}
        onFilterChange={(f) => (activityFilter = f)}
        onRetry={loadActivity}
      />
    {:else}
      <InboxNotificationsTab
        logs={notificationLogs}
        {channels}
        loading={notificationsLoading}
        error={notificationsError}
        {channelsError}
        onRetry={loadNotificationLogs}
      />
    {/if}

    <RejectReasonDialog
      open={rejectTarget !== null}
      {submitting}
      title={$t("inbox.reject_title")}
      onclose={() => (rejectTarget = null)}
      onconfirm={confirmReject}
    />
  </div>
</RouteTransition>
