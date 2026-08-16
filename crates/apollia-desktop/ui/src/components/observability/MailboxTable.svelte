<script lang="ts">
  import { onMount } from "svelte";
  import { t, locale } from "svelte-i18n";
  import { Card } from "$lib/components/ui/card";
  import { Mailbox, ArrowRight } from "lucide-svelte";
  import { listMailboxMessages, type MailboxMessageRow } from "$lib/ipc/mailbox";

  const PAGE_SIZE = 100;

  let rows = $state<MailboxMessageRow[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);

  /** Chip classes per delivery state. `in_flight` reads as a warm, active lease;
   *  `pending` as a neutral queued message. Falls back to a muted chip. */
  const STATE_CHIP: Record<string, string> = {
    pending: "bg-info/10 text-info border-info/30",
    in_flight: "bg-warning/10 text-warning border-warning/30",
  };

  function stateChip(state: string): string {
    return STATE_CHIP[state] ?? "glass-border-subtle glass-inset text-muted-foreground";
  }

  function stateLabel(state: string): string {
    if (state === "pending") return $t("observability.mailbox.state_pending");
    if (state === "in_flight") return $t("observability.mailbox.state_in_flight");
    return state;
  }

  function formatTimestamp(iso: string): string {
    if (!iso) return "";
    const d = new Date(iso);
    if (Number.isNaN(d.getTime())) return iso;
    return d.toLocaleString($locale ?? "en", {
      day: "2-digit",
      month: "short",
      hour: "2-digit",
      minute: "2-digit",
    });
  }

  async function loadMessages(): Promise<void> {
    try {
      rows = await listMailboxMessages(PAGE_SIZE);
      error = null;
    } catch (err: unknown) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    void loadMessages();
  });
</script>

<div class="space-y-5">
  <div class="space-y-1">
    <h3 class="m-0 text-heading-sm font-semibold text-foreground">
      {$t("observability.mailbox.title")}
    </h3>
    <p class="text-body-sm text-muted-foreground">
      {$t("observability.mailbox.intro")}
    </p>
  </div>

  {#if loading}
    <p class="text-sm text-muted-foreground" data-testid="mailbox-loading">
      {$t("observability.mailbox.loading")}
    </p>
  {:else if error}
    <p class="text-sm text-destructive">{error}</p>
  {:else if rows.length === 0}
    <Card
      class="flex flex-col items-center justify-center py-16"
      data-testid="mailbox-empty"
    >
      <div class="rounded-full glass-inset p-4 mb-4">
        <Mailbox class="h-8 w-8 text-muted-foreground/60" />
      </div>
      <p class="text-body-sm text-muted-foreground">{$t("observability.mailbox.empty")}</p>
    </Card>
  {:else}
    <Card class="overflow-hidden" data-testid="mailbox-list">
      <div class="overflow-x-auto">
        <table class="w-full min-w-[720px] text-body-sm">
          <thead>
            <tr class="border-b border-border/40">
              <th
                scope="col"
                class="section-meta text-left px-5 py-3"
              >
                {$t("observability.mailbox.col_from")}
              </th>
              <th
                scope="col"
                class="section-meta text-left px-3 py-3"
              >
                {$t("observability.mailbox.col_to")}
              </th>
              <th
                scope="col"
                class="section-meta text-left px-3 py-3"
              >
                {$t("observability.mailbox.col_payload")}
              </th>
              <th
                scope="col"
                class="section-meta text-left px-3 py-3"
              >
                {$t("observability.mailbox.col_state")}
              </th>
              <th
                scope="col"
                class="section-meta text-right px-5 py-3"
              >
                {$t("observability.mailbox.col_sent_at")}
              </th>
            </tr>
          </thead>
          <tbody>
            {#each rows as row (row.message_id)}
              <tr
                class="border-b border-border/20 last:border-0 align-top"
                data-testid="mailbox-message-row"
              >
                <td class="px-5 py-3">
                  <span
                    class="inline-flex items-center gap-1.5 font-medium text-foreground"
                    data-testid="mailbox-message-from"
                  >
                    <ArrowRight class="h-3 w-3 text-muted-foreground/50" aria-hidden="true" />
                    {row.from_agent}
                  </span>
                </td>
                <td class="px-3 py-3">
                  <span class="font-medium text-foreground" data-testid="mailbox-message-to">
                    {row.to_agent}
                  </span>
                </td>
                <td class="px-3 py-3 max-w-[24rem]">
                  <pre
                    class="overflow-x-auto whitespace-pre-wrap break-words text-code-sm font-mono leading-relaxed text-muted-foreground"
                    data-testid="mailbox-message-payload">{row.payload}</pre>
                </td>
                <td class="px-3 py-3">
                  <span
                    class="inline-flex items-center rounded-full border px-2 py-[1px] text-caption font-medium {stateChip(row.state)}"
                    data-testid="mailbox-message-state"
                  >
                    {stateLabel(row.state)}
                  </span>
                </td>
                <td class="px-5 py-3 text-right text-caption text-muted-foreground tabular-nums">
                  {formatTimestamp(row.sent_at)}
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    </Card>
  {/if}
</div>
