<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import { Inbox as InboxIcon } from "lucide-svelte";

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
  import type { ResolvedChatApproval } from "$lib/types";
  import { CheckCircle2, XCircle, ShieldCheck } from "lucide-svelte";

  import type { InboxItem, InboxRisk } from "../components/inbox/types";
  import type {
    PendingApproval,
    PendingChatApproval,
    PendingUserInputView,
  } from "$lib/types";

  // ── State ────────────────────────────────────────────────────────────────
  let loading = $state(true);
  let error = $state<string | null>(null);
  let submitting = $state(false);
  let expandedId = $state<string | null>(null);
  let rejectTarget = $state<InboxItem | null>(null);
  let history = $state<ResolvedChatApproval[]>([]);
  let historyError = $state<string | null>(null);

  type FilterKey =
    | "all"
    | "approval"
    | "deliverable"
    | "trigger"
    | "error"
    | "memory"
    | "cost";
  let activeFilter = $state<FilterKey>("all");

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
    let questions: unknown[] = [];
    try {
      questions = JSON.parse(u.questions_json);
    } catch {
      /* ignore */
    }
    const firstQ =
      (questions[0] as { question?: string } | undefined)?.question ?? "Question";
    return {
      id: `ask_user:${u.request_id}`,
      kind: "ask_user" as const,
      agentName: u.session_id ? u.session_id.slice(0, 8) : "agent",
      sessionId: u.session_id || undefined,
      summary: firstQ.slice(0, 140),
      suspendedAt: u.created_at,
      source: u,
      questions,
    };
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

  // Map every InboxItem to the V3 InboxRow type.
  function rowType(item: InboxItem): InboxType {
    // All current backend kinds surface as approvals in the V3 design.
    // memory / cost / trigger / error / deliverable categories will be
    // populated as new event sources land.
    void item;
    return "approval";
  }

  function rowFilterKey(item: InboxItem): FilterKey {
    return rowType(item) as FilterKey;
  }

  const filteredItems = $derived.by(() => {
    if (activeFilter === "all") return allItems;
    return allItems.filter((i) => rowFilterKey(i) === activeFilter);
  });

  // Counts per filter (for chip labels).
  const counts = $derived.by(() => {
    const c: Record<FilterKey, number> = {
      all: allItems.length,
      approval: 0,
      deliverable: 0,
      trigger: 0,
      error: 0,
      memory: 0,
      cost: 0,
    };
    for (const i of allItems) {
      const k = rowFilterKey(i);
      if (k in c) c[k] += 1;
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
  const GROUP_LABEL: Record<GroupKey, string> = {
    today: "Aujourd'hui",
    yesterday: "Hier",
    earlier: "Plus tôt",
  };

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
    // Backend doesn't currently expose a deterministic expiry — leave blank.
    return undefined;
  }

  // ── Lifecycle ────────────────────────────────────────────────────────────
  onMount(() => {
    requestNotificationPermission();
    void loadData();
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
      if (!approved && reason) {
        await invoke("respond_user_input_rejected", {
          requestId: item.source.request_id,
          reason,
        });
      } else {
        await invoke("respond_user_input", {
          requestId: item.source.request_id,
          answers: [],
        });
      }
    } else {
      // Chat tool approval — forward the operator-provided reason so the
      // builtin agent can surface it to the LLM on the next iteration.
      await invoke("authorize_chat_tool", {
        sessionId: item.source.sessionId,
        messageId: item.source.messageId,
        toolName: item.source.toolName,
        decision: approved ? "accept" : "refuse",
        reason: reason ?? null,
      });
    }
  }

  /** "Toujours autoriser" path for chat tool approvals (not applicable to
   *  task-level pauses or ask_user). */
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

  // ── Filter chip definitions ──────────────────────────────────────────────
  const FILTERS: { key: FilterKey; label: string }[] = [
    { key: "all", label: "Tous" },
    { key: "approval", label: "Approbations" },
    { key: "deliverable", label: "Livrables" },
    { key: "trigger", label: "Triggers" },
    { key: "error", label: "Erreurs" },
    { key: "memory", label: "Mémoire" },
    { key: "cost", label: "Coût" },
  ];
</script>

<div class="flex h-full min-h-0 w-full flex-col overflow-hidden" data-testid="inbox-page">
  <PageHeader
    kicker="BOÎTE DE RÉCEPTION"
    title="Boîte de réception"
    subtitle={totalPending > 0
      ? $t("inbox.pending_count", { values: { count: totalPending } })
      : $t("inbox.subtitle")}
  />

  <!-- Filter chips ---------------------------------------------------- -->
  <div class="flex flex-wrap items-center gap-1.5 px-8 pt-4 pb-2">
    {#each FILTERS as f (f.key)}
      {@const isActive = activeFilter === f.key}
      <button
        type="button"
        class="rounded-full text-[11px] font-medium transition-colors px-2.5 py-1 border {isActive
          ? 'bg-primary/10 text-primary border-primary/20'
          : 'bg-transparent text-muted-foreground border-border hover:bg-muted/40'}"
        onclick={() => (activeFilter = f.key)}
        aria-pressed={isActive}
        data-testid="inbox-filter-{f.key}"
      >
        {f.label} · {counts[f.key]}
      </button>
    {/each}
  </div>

  <!-- Body ------------------------------------------------------------ -->
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
          <SectionTitle count={grouped[g].length}>{GROUP_LABEL[g]}</SectionTitle>
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
                    onAction={isApproval
                      ? (e) => {
                          e.stopPropagation?.();
                          toggleExpand(item);
                        }
                      : undefined}
                  />
                  {#if isExpanded && isApproval}
                    <div class="px-4 py-3 border-b border-border bg-muted/20">
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
                    </div>
                  {/if}
                </div>
              {/each}
            </div>
          </div>
        {/if}
      {/each}
    {/if}

    <!-- Historique des décisions résolues (lecture seule) ----------------- -->
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
                        {#if isAccept}Autorisé{:else if isAlways}Toujours autorisé{:else}Refusé{/if}
                      </span>
                    </div>
                    <span class="text-[10.5px] text-muted-foreground/70 font-mono shrink-0" title={h.resolved_at}>
                      {relTime(h.resolved_at)}
                    </span>
                  </div>
                  {#if isRefuse && h.reason}
                    <p class="mt-0.5 text-[11px] text-destructive/80 line-clamp-2" title={h.reason}>
                      <span class="font-medium">Raison :</span> {h.reason}
                    </p>
                  {/if}
                  <p class="mt-0.5 text-[10.5px] text-muted-foreground/60">
                    Session <code class="font-mono">{h.session_id.slice(0, 8)}</code>
                  </p>
                </div>
              </li>
            {/each}
          </ul>
        {/if}
      </div>
    {/if}
  </div>

  <RejectReasonDialog
    open={rejectTarget !== null}
    {submitting}
    title={$t("inbox.reject_title")}
    onclose={() => (rejectTarget = null)}
    onconfirm={confirmReject}
  />
</div>
