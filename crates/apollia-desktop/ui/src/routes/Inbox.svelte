<script lang="ts">
  /**
   * "To review": every decision waiting for the user in one list, answered
   * with the cards of the chat. The list is the runtime's own gathering of the
   * consents of the conversations, the task pauses, the batch items and the
   * questions of agents; answering here goes through the call that answers
   * each where it was born, so the conversation, the task or the batch goes on.
   *
   * There is no history here: a decision taken is read in the audit journal.
   */
  import { onMount } from "svelte";
  import { t, locale } from "svelte-i18n";
  import { ShieldCheck } from "lucide-svelte";
  import { formatRelativeTime } from "$lib/utils";

  import { pendingApprovals, requestNotificationPermission } from "$lib/stores/hitl";
  import {
    addPendingUserInput,
    pendingChatApprovals,
    pendingChatSessionId,
    pendingUserInputs,
  } from "$lib/stores/chat";
  import { chatSessions } from "$lib/stores/sse";
  import { reportError } from "$lib/errors/reportError";
  import { navigateTo } from "$lib/stores/navigation";
  import { requestSheet } from "$lib/stores/agentOpen";
  import { openBatch } from "$lib/stores/batchView";

  import { Banner } from "$lib/components/ui/banner";
  import RouteTransition from "$lib/components/ui/route-transition/RouteTransition.svelte";
  import { EmptyState, FilterChipBar, PageHeader } from "$lib/components/operator";

  import InboxErrorBox from "../components/inbox/InboxErrorBox.svelte";
  import ReviewItem from "../components/inbox/ReviewItem.svelte";
  import { createAnswerHold } from "../components/inbox/answerHold";
  import {
    ANSWER_HOLD_MS,
    REVIEW_FILTERS,
    filterCounts,
    newestFirst,
    originOf,
    shownUnder,
    type ReviewFilter,
  } from "../components/inbox/reviewModel";

  import { listPendingApprovals, listPendingUserInputs } from "$lib/ipc/inbox";
  import {
    decidePendingDecision,
    type DecideRequest,
    type PendingDecision,
  } from "$lib/ipc/pendingDecisions";
  import { pendingDecisions, refreshPendingDecisions } from "$lib/stores/pendingDecisions";

  /** How often the list is read again while the screen is open. */
  const REFRESH_MS = 15_000;

  // The list the sidebar counts: one read, shared, so the two always agree.
  const items = $derived($pendingDecisions);
  let loading = $state(true);
  let error = $state<unknown>(null);
  let filter = $state<ReviewFilter>("all");
  /** The answers given and not sent yet, by item id. */
  let held = $state<Record<string, DecideRequest>>({});
  /** Batch items that ran again thanks to the last "always" answered here. */
  let resumedBatchItems = $state(0);

  const relTime = (iso: string) => formatRelativeTime(iso, $locale ?? "en");

  const ordered = $derived(newestFirst(items));
  /** What still waits: an item answered here no longer counts. */
  const waiting = $derived(ordered.filter((item) => !(item.id in held)));
  const counts = $derived(filterCounts(waiting));
  const shown = $derived(shownUnder(ordered, filter));
  const chips = $derived(
    REVIEW_FILTERS.map((key) => ({
      key,
      label: $t(`review.filter.${key}`),
      count: counts[key],
    })),
  );
  const titles = $derived(
    new Map($chatSessions.map((session) => [session.id, session.title] as const)),
  );

  function conversationTitle(item: PendingDecision): string | null {
    const origin = originOf(item);
    return origin.kind === "conversation" ? (titles.get(origin.sessionId) ?? null) : null;
  }

  const hold = createAnswerHold<DecideRequest>({
    delayMs: ANSWER_HOLD_MS,
    send: async (_id, request) => {
      const response = await decidePendingDecision(request);
      resumedBatchItems = response.batchItemsResumed;
    },
    settled: (id, failure) => {
      const { [id]: _sent, ...rest } = held;
      held = rest;
      if (failure) reportError(failure, { surface: "toast" });
      void load();
    },
  });

  function answer(item: PendingDecision, request: DecideRequest): void {
    resumedBatchItems = 0;
    held = { ...held, [item.id]: request };
    hold.hold(item.id, request);
  }

  function cancel(item: PendingDecision): void {
    hold.cancel(item.id);
    const { [item.id]: _cancelled, ...rest } = held;
    held = rest;
  }

  function open(item: PendingDecision): void {
    const origin = originOf(item);
    if (origin.kind === "conversation") {
      pendingChatSessionId.set(origin.sessionId);
      navigateTo("chat");
    } else if (origin.kind === "batch") {
      openBatch(origin.batchId, "history");
    } else if (origin.kind === "task" && item.agentId) {
      requestSheet({ kind: "agent", id: item.agentId });
    }
  }

  async function load(): Promise<void> {
    try {
      const listed = await refreshPendingDecisions();
      error = null;
      // An item answered elsewhere meanwhile has nothing left to send.
      const present = new Set(listed.map((item) => item.id));
      for (const id of Object.keys(held)) {
        if (!present.has(id) && hold.cancel(id)) {
          const { [id]: _gone, ...rest } = held;
          held = rest;
        }
      }
    } catch (e) {
      error = e;
    } finally {
      loading = false;
    }
  }

  /**
   * The counters of the sidebar and of the chat read the stores the events
   * feed; they are seeded here as well, so a decision that waited before this
   * window opened is counted.
   */
  async function seedCounters(): Promise<void> {
    const [pauses, questions] = await Promise.allSettled([
      listPendingApprovals(),
      listPendingUserInputs(),
    ]);
    if (pauses.status === "fulfilled") pendingApprovals.set(pauses.value);
    if (questions.status === "fulfilled") {
      for (const question of questions.value) addPendingUserInput(question);
    }
  }

  // A decision asked or answered elsewhere moves the stores the events feed:
  // the list is read again when one of them changes.
  $effect(() => {
    void $pendingApprovals.length;
    void $pendingChatApprovals.length;
    void $pendingUserInputs.length;
    void load();
  });

  onMount(() => {
    requestNotificationPermission();
    void seedCounters();
    const refresh = setInterval(() => void load(), REFRESH_MS);
    return () => {
      clearInterval(refresh);
      hold.flush();
    };
  });
</script>

<RouteTransition>
  <div class="flex h-full min-h-0 w-full flex-col overflow-hidden" data-testid="inbox-page">
    <PageHeader
      title={$t("review.title")}
      subtitle={waiting.length > 0
        ? `${$t(waiting.length === 1 ? "review.count_one" : "review.count_other", {
            values: { count: waiting.length },
          })} ${$t("review.subtitle")}`
        : $t("review.subtitle")}
    />

    {#if error}
      <div class="px-8 pt-5">
        <InboxErrorBox {error} onRetry={load} testid="review-error" />
      </div>
    {/if}

    {#if !loading && items.length === 0 && !error}
      <div class="flex min-h-0 flex-1 items-center justify-center" data-testid="review-empty">
        <EmptyState title={$t("review.empty.title")} desc={$t("review.empty.desc")} tone="neutral">
          {#snippet icon()}
            <ShieldCheck class="h-6 w-6" />
          {/snippet}
        </EmptyState>
      </div>
    {:else if items.length > 0}
      <FilterChipBar
        {chips}
        activeKey={filter}
        onchange={(key) => (filter = key as ReviewFilter)}
        testidPrefix="review-filter"
        data-testid="review-filters"
      />

      <div class="min-h-0 flex-1 overflow-y-auto px-8 pb-8" data-testid="review-list">
        <div class="mx-auto flex max-w-3xl flex-col gap-6">
          {#if resumedBatchItems > 0}
            <Banner variant="success" surface="card" data-testid="review-resumed">
              {$t(resumedBatchItems === 1 ? "review.resumed_one" : "review.resumed_other", {
                values: { count: resumedBatchItems },
              })}
            </Banner>
          {/if}
          {#each shown as item (item.id)}
            <ReviewItem
              {item}
              conversationTitle={conversationTitle(item)}
              held={held[item.id] ?? null}
              {relTime}
              onanswer={answer}
              oncancel={cancel}
              onopen={open}
            />
          {/each}
        </div>
      </div>
    {/if}
  </div>
</RouteTransition>
