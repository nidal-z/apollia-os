<script lang="ts">
  /**
   * ConnectionActivitySection - what the agents did with a connection over the
   * last seven days: when it was last used, how many actions each agent took,
   * then "See in the journal", which opens the journal narrowed to it. The
   * journal holds the counts; the section never reads a content.
   */
  import { t } from "svelte-i18n";
  import { ArrowRight, Bot } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { Card } from "$lib/components/ui/card";
  import { createIdentityGuard } from "$lib/utils/identityGuard";
  import { messageOf } from "$lib/connections/errors";
  import {
    ACTIVITY_DAYS,
    activitySince,
    toolPrefixOf,
    usedDescriptor,
  } from "$lib/connections/connectionSections";
  import { getConnectionActivity, type ConnectionActivity } from "$lib/ipc/connections";
  import { APOLLIA_AGENT_ID } from "$lib/chat/chatTargets";
  import { requestJournalConnection } from "$lib/stores/journalFocus";
  import { navigateTo } from "$lib/stores/navigation";
  import type { Connection } from "$lib/types";

  interface Props {
    connection: Connection;
    /** The name the list gives it, carried to the journal. */
    label: string;
  }

  let { connection, label }: Props = $props();

  let activity = $state<ConnectionActivity | null>(null);
  let error = $state<string | null>(null);
  let now = $state(Date.now());

  const prefix = $derived(toolPrefixOf(connection));
  const lastUsed = $derived(usedDescriptor(activity?.lastUsedAt ?? null, now));
  const guard = createIdentityGuard(() => connection.id);

  function nameOf(agent: string | null): string {
    if (agent === null) return $t("connections.card.activity.unknown_agent");
    return agent === APOLLIA_AGENT_ID ? $t("authorizations.apollia_name") : agent;
  }

  $effect(() => {
    const at = prefix;
    void connection.id;
    activity = null;
    error = null;
    if (at === null) return;
    const ticket = guard.begin();
    now = Date.now();
    getConnectionActivity(at, activitySince(now))
      .then((answer) => {
        if (ticket.current) activity = answer ?? null;
      })
      .catch((err: unknown) => {
        if (ticket.current) error = messageOf(err);
      });
  });

  function openJournal(): void {
    if (prefix === null) return;
    requestJournalConnection({ prefix, label });
    navigateTo("journal");
  }
</script>

<Card class="max-w-3xl space-y-2.5 px-4 py-3.5" data-testid="connection-activity">
  <div class="flex items-center justify-between">
    <span class="text-label-md font-semibold text-foreground">
      {$t("connections.card.activity.title", { values: { days: ACTIVITY_DAYS } })}
    </span>
  </div>

  {#if prefix === null}
    <p class="text-caption text-muted-foreground" data-testid="connection-activity-shared">
      {$t("connections.card.activity.mailbox")}
    </p>
  {:else if error}
    <p class="text-caption text-destructive" data-testid="connection-activity-error">{error}</p>
  {:else if activity !== null && activity.actions === 0}
    <p class="text-caption text-muted-foreground" data-testid="connection-activity-none">
      {$t("connections.card.activity.none", { values: { days: ACTIVITY_DAYS } })}
    </p>
  {:else if activity !== null}
    {#if lastUsed}
      <p class="text-body-xs text-foreground" data-testid="connection-activity-last">
        {$t(lastUsed.key, { values: lastUsed.values })}
      </p>
    {/if}
    <ul class="space-y-1">
      {#each activity.agents as row (row.agent ?? "")}
        <li class="flex items-center gap-2 text-body-xs text-foreground" data-testid="connection-activity-agent">
          <Bot size={12} class="shrink-0 text-muted-foreground" aria-hidden="true" />
          <span class="min-w-0 flex-1 truncate">{nameOf(row.agent)}</span>
          <span class="text-caption text-muted-foreground">
            {$t("connections.card.activity.actions", { values: { count: row.actions } })}
          </span>
        </li>
      {/each}
    </ul>
  {/if}

  {#if prefix !== null}
    <Button variant="ghost" size="sm" onclick={openJournal} data-testid="connection-activity-journal">
      {$t("connections.card.activity.journal")}
      <ArrowRight size={12} aria-hidden="true" />
    </Button>
  {/if}
</Card>
