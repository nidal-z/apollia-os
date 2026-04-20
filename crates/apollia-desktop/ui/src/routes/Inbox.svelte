<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import { uiMode } from "$lib/stores/mode";
  import { pendingApprovals, pendingCount, requestNotificationPermission } from "$lib/stores/hitl";
  import { pendingChatApprovals, pendingChatApprovalCount } from "$lib/stores/chat";
  import {
    selectedIds,
    selectionCount,
    toggle as toggleSelection,
    clear as clearSelection,
    selectAll,
  } from "$lib/stores/inbox/selection";
  import { Sheet } from "$lib/components/ui/sheet";
  import { Badge } from "$lib/components/ui/badge";
  import { addToast } from "$lib/components/ui/toast/store";
  import EmptyState from "../components/common/EmptyState.svelte";
  import { ShieldCheck } from "lucide-svelte";
  import InboxList from "../components/inbox/InboxList.svelte";
  import InboxFilters, { type StatusFilter } from "../components/inbox/InboxFilters.svelte";
  import InboxPreview from "../components/inbox/InboxPreview.svelte";
  import InboxBulkBar from "../components/inbox/InboxBulkBar.svelte";
  import RejectReasonDialog from "../components/inbox/RejectReasonDialog.svelte";
  import ConfirmDialog from "$lib/components/ui/dialog/ConfirmDialog.svelte";
  import type { InboxItem, InboxItemKind, InboxRisk } from "../components/inbox/types";
  import type { PendingApproval, PendingChatApproval } from "$lib/types";

  // ── State ──────────────────────────────────────────────────────────────────

  let loading = $state(true);
  let error = $state<string | null>(null);
  let activeId = $state<string | null>(null);
  let mobileSheetOpen = $state(false);
  let isMobile = $state(false);
  let submitting = $state(false);

  // Filters (persisted in URL hash for simple cross-session continuity).
  let search = $state("");
  let kinds = $state<Set<InboxItemKind>>(new Set());
  let statusFilter = $state<StatusFilter>("pending");
  let agentFilter = $state<string | null>(null);

  // ── Adapters ───────────────────────────────────────────────────────────────

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
      consequences: Array.isArray(rec.consequences) ? (rec.consequences as string[]) : undefined,
      rationale: typeof rec.rationale === "string" ? rec.rationale : undefined,
      thinking: typeof rec.thinking === "string" ? rec.thinking : undefined,
    };
  }

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

  // ── Derived inbox stream ───────────────────────────────────────────────────

  const allItems = $derived.by<InboxItem[]>(() => {
    const task = $pendingApprovals.map(taskToInbox);
    const chat = $pendingChatApprovals.map(chatToInbox);
    return [...task, ...chat].sort(
      (a, b) => new Date(a.suspendedAt).getTime() - new Date(b.suspendedAt).getTime(),
    );
  });

  const agents = $derived.by(() => Array.from(new Set(allItems.map((i) => i.agentName))).sort());

  const filteredItems = $derived.by(() => {
    const q = search.trim().toLowerCase();
    return allItems.filter((i) => {
      if (kinds.size > 0 && !kinds.has(i.kind)) return false;
      if (agentFilter && i.agentName !== agentFilter) return false;
      if (q) {
        const hay = `${i.summary} ${i.toolName ?? ""} ${i.agentName}`.toLowerCase();
        if (!hay.includes(q)) return false;
      }
      return true;
    });
  });

  const visibleIds = $derived(filteredItems.map((i) => i.id));
  const activeItem = $derived(filteredItems.find((i) => i.id === activeId) ?? null);

  const highRiskSelectedCount = $derived.by(() => {
    let n = 0;
    for (const item of filteredItems) {
      if ($selectedIds.has(item.id) && item.risk?.level === "high") n += 1;
    }
    return n;
  });

  const totalPending = $derived($pendingCount + $pendingChatApprovalCount);

  // Reconcile active id when items change.
  $effect(() => {
    if (activeId && !allItems.some((i) => i.id === activeId)) {
      activeId = null;
    }
    if (!activeId && filteredItems.length > 0 && !isMobile) {
      activeId = filteredItems[0].id;
    }
  });

  // ── URL hash persistence ───────────────────────────────────────────────────

  function readHash() {
    if (typeof window === "undefined") return;
    const raw = window.location.hash.startsWith("#inbox?")
      ? window.location.hash.slice("#inbox?".length)
      : "";
    if (!raw) return;
    const params = new URLSearchParams(raw);
    const type = params.get("type");
    if (type) kinds = new Set(type.split(",") as InboxItemKind[]);
    const status = params.get("status");
    if (status === "pending" || status === "resolved" || status === "all") statusFilter = status;
    const agent = params.get("agent");
    if (agent) agentFilter = agent;
    const q = params.get("q");
    if (q) search = q;
    const item = params.get("item");
    if (item) activeId = item;
  }

  function writeHash() {
    if (typeof window === "undefined") return;
    const params = new URLSearchParams();
    if (kinds.size > 0) params.set("type", Array.from(kinds).join(","));
    if (statusFilter !== "pending") params.set("status", statusFilter);
    if (agentFilter) params.set("agent", agentFilter);
    if (search) params.set("q", search);
    if (activeId) params.set("item", activeId);
    const s = params.toString();
    const next = s ? `#inbox?${s}` : "#inbox";
    if (window.location.hash !== next) {
      history.replaceState(null, "", next);
    }
  }

  $effect(() => {
    // Re-runs whenever any filter/active changes.
    // eslint-disable-next-line @typescript-eslint/no-unused-expressions
    [kinds, statusFilter, agentFilter, search, activeId];
    writeHash();
  });

  // ── Lifecycle ──────────────────────────────────────────────────────────────

  onMount(() => {
    requestNotificationPermission();
    readHash();
    const mq = window.matchMedia("(max-width: 1023px)");
    isMobile = mq.matches;
    const handler = (e: MediaQueryListEvent) => { isMobile = e.matches; };
    mq.addEventListener("change", handler);
    void loadData();
    return () => mq.removeEventListener("change", handler);
  });

  async function loadData(): Promise<void> {
    loading = true;
    error = null;
    try {
      const pending = await invoke<PendingApproval[]>("list_pending_approvals");
      pendingApprovals.set(pending);
    } catch (err: unknown) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      loading = false;
    }
  }

  // ── Handlers ───────────────────────────────────────────────────────────────

  function handleSelectItem(id: string) {
    activeId = id;
    if (isMobile) mobileSheetOpen = true;
  }

  function handleToggleKind(k: InboxItemKind) {
    const next = new Set(kinds);
    if (next.has(k)) next.delete(k);
    else next.add(k);
    kinds = next;
  }

  async function resolveItem(item: InboxItem, approved: boolean, reason?: string) {
    if (item.kind === "task") {
      await invoke("resume_task", {
        taskId: item.source.task_id,
        approved,
        reason: reason ?? null,
      });
    } else {
      await invoke("authorize_chat_tool", {
        sessionId: item.source.sessionId,
        messageId: item.source.messageId,
        toolName: item.source.toolName,
        decision: approved ? "accept" : "refuse",
      });
    }
  }

  async function handleAccept(item: InboxItem) {
    submitting = true;
    try {
      await resolveItem(item, true);
      addToast($t("inbox.toast.accepted"), "success");
    } catch (err: unknown) {
      addToast(err instanceof Error ? err.message : String(err), "error");
    } finally {
      submitting = false;
    }
  }

  async function handleAlwaysAccept(item: InboxItem) {
    submitting = true;
    try {
      if (item.kind === "task") {
        await resolveItem(item, true);
      } else {
        await invoke("authorize_chat_tool", {
          sessionId: item.source.sessionId,
          messageId: item.source.messageId,
          toolName: item.source.toolName,
          decision: "always_accept",
        });
      }
      addToast($t("inbox.toast.always_accepted"), "success");
    } catch (err: unknown) {
      addToast(err instanceof Error ? err.message : String(err), "error");
    } finally {
      submitting = false;
    }
  }

  // Reject flow (single + bulk)
  let rejectTarget = $state<InboxItem | null>(null);
  let bulkRejectOpen = $state(false);
  let bulkApproveConfirmOpen = $state(false);

  function openReject(item: InboxItem) {
    rejectTarget = item;
  }

  async function confirmReject(reason: string) {
    if (!rejectTarget) return;
    submitting = true;
    try {
      await resolveItem(rejectTarget, false, reason);
      addToast($t("inbox.toast.rejected"), "success");
      rejectTarget = null;
    } catch (err: unknown) {
      addToast(err instanceof Error ? err.message : String(err), "error");
    } finally {
      submitting = false;
    }
  }

  async function confirmBulkReject(reason: string) {
    submitting = true;
    const ids = Array.from($selectedIds);
    try {
      for (const id of ids) {
        const it = allItems.find((x) => x.id === id);
        if (it) await resolveItem(it, false, reason);
      }
      addToast($t("inbox.toast.bulk_rejected", { values: { count: ids.length } }), "success");
      clearSelection();
      bulkRejectOpen = false;
    } catch (err: unknown) {
      addToast(err instanceof Error ? err.message : String(err), "error");
    } finally {
      submitting = false;
    }
  }

  async function confirmBulkApprove() {
    submitting = true;
    const ids = Array.from($selectedIds);
    try {
      for (const id of ids) {
        const it = allItems.find((x) => x.id === id);
        if (it) await resolveItem(it, true);
      }
      addToast($t("inbox.toast.bulk_approved", { values: { count: ids.length } }), "success");
      clearSelection();
      bulkApproveConfirmOpen = false;
    } catch (err: unknown) {
      addToast(err instanceof Error ? err.message : String(err), "error");
    } finally {
      submitting = false;
    }
  }

  function tryBulkApprove() {
    if ($selectionCount === 0) return;
    bulkApproveConfirmOpen = true;
  }
</script>

<div class="mx-auto flex h-full w-full max-w-[1400px] flex-col gap-4" data-testid="inbox-page">
  <!-- Header -->
  <header class="flex items-center justify-between">
    <div>
      <div class="flex items-center gap-3">
        <h1 class="text-2xl font-semibold" data-testid="inbox-title">
          {$uiMode === "operator" ? $t("inbox.title_operator") : $t("inbox.title_builder")}
        </h1>
        {#if totalPending > 0}
          <Badge variant="destructive" data-testid="inbox-pending-count">
            {$t("inbox.pending_count", { values: { count: totalPending } })}
          </Badge>
        {/if}
      </div>
      <p class="mt-1 text-xs text-muted-foreground">{$t("inbox.subtitle")}</p>
    </div>
  </header>

  <!-- Filters -->
  <InboxFilters
    {search}
    {kinds}
    status={statusFilter}
    agents={agents}
    selectedAgent={agentFilter}
    onsearch={(v) => (search = v)}
    ontogglekind={handleToggleKind}
    onstatuschange={(s) => (statusFilter = s)}
    onagentchange={(a) => (agentFilter = a)}
  />

  {#if loading}
    <p class="text-sm text-muted-foreground">{$t("common.loading")}</p>
  {:else if error}
    <p class="text-sm text-destructive">{error}</p>
  {:else if totalPending === 0}
    <EmptyState
      icon={ShieldCheck}
      title={$t("inbox.empty_title")}
      subtitle={$t("inbox.empty_subtitle")}
      page="approvals"
    />
  {:else}
    <!-- Desktop: 2-pane. Mobile: list only, preview in bottom sheet. -->
    <div
      class="relative flex min-h-0 flex-1 flex-col gap-4 lg:flex-row"
      data-testid="inbox-layout"
    >
      <aside
        class="flex min-h-0 flex-col overflow-hidden rounded-lg border border-border bg-card lg:w-[380px] lg:shrink-0"
        aria-label={$t("inbox.list_aria")}
      >
        <InboxList
          items={filteredItems}
          selectedIds={$selectedIds}
          {activeId}
          onselect={handleSelectItem}
          ontoggle={(id) => toggleSelection(id)}
          onselectAll={() => selectAll(visibleIds)}
          onclearSelection={clearSelection}
        />
      </aside>

      <section
        class="hidden min-h-0 flex-1 overflow-hidden rounded-lg border border-border bg-card lg:flex shadow-[0_8px_24px_rgba(0,0,0,0.08)]"
        aria-label={$t("inbox.preview_aria")}
      >
        <InboxPreview
          item={activeItem}
          {submitting}
          onaccept={handleAccept}
          onreject={openReject}
          onalwaysAccept={handleAlwaysAccept}
        />
      </section>
    </div>

    <InboxBulkBar
      count={$selectionCount}
      highRiskCount={highRiskSelectedCount}
      {submitting}
      onapprove={tryBulkApprove}
      onreject={() => (bulkRejectOpen = true)}
      oncancel={clearSelection}
    />
  {/if}

  <!-- Mobile preview sheet -->
  <Sheet open={mobileSheetOpen && isMobile} onclose={() => (mobileSheetOpen = false)} width="lg">
    <div class="flex h-full flex-col">
      <InboxPreview
        item={activeItem}
        {submitting}
        onaccept={handleAccept}
        onreject={openReject}
        onalwaysAccept={handleAlwaysAccept}
      />
    </div>
  </Sheet>

  <!-- Single-item reject dialog -->
  <RejectReasonDialog
    open={rejectTarget !== null}
    submitting={submitting}
    title={$t("inbox.reject_title")}
    onclose={() => (rejectTarget = null)}
    onconfirm={confirmReject}
  />

  <!-- Bulk reject dialog -->
  <RejectReasonDialog
    open={bulkRejectOpen}
    submitting={submitting}
    title={$t("inbox.bulk_reject_title", { values: { count: $selectionCount } })}
    onclose={() => (bulkRejectOpen = false)}
    onconfirm={confirmBulkReject}
  />

  <!-- Bulk approve confirm -->
  <ConfirmDialog
    open={bulkApproveConfirmOpen}
    title={$t("inbox.bulk_approve_title", { values: { count: $selectionCount } })}
    message={highRiskSelectedCount > 0
      ? $t("inbox.bulk_approve_warning", { values: { count: highRiskSelectedCount } })
      : $t("inbox.bulk_approve_confirm_body", { values: { count: $selectionCount } })}
    confirmLabel={$t("inbox.bulk_approve", { values: { count: $selectionCount } })}
    onclose={() => (bulkApproveConfirmOpen = false)}
    onconfirm={confirmBulkApprove}
  />
</div>
