<script lang="ts">
  /**
   * ConnectionAgentsSection - the agents that use a connection, as the runtime
   * grants them, and "Give to an agent": a list to check, every tool of the
   * connection given to each agent checked, in one grant.
   */
  import { t } from "svelte-i18n";
  import { Bot, Plus } from "lucide-svelte";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { Card } from "$lib/components/ui/card";
  import { Checkbox } from "$lib/components/ui/checkbox";
  import { createIdentityGuard } from "$lib/utils/identityGuard";
  import { messageOf } from "$lib/connections/errors";
  import { agentsToOffer, grantedTools, type AgentChoice } from "$lib/connections/connectionSections";
  import {
    getConnectionAgents,
    grantConnection,
    listAgents,
    type ConnectionAgent,
  } from "$lib/ipc/connections";
  import { APOLLIA_AGENT_ID } from "$lib/chat/chatTargets";
  import type { Connection } from "$lib/types";

  interface Props {
    connection: Connection;
  }

  let { connection }: Props = $props();

  let holders = $state<ConnectionAgent[]>([]);
  let sharedWith = $state<string[]>([]);
  let loadError = $state<string | null>(null);
  /** The agents offered, while "Give to an agent" is open. */
  let offer = $state<AgentChoice[] | null>(null);
  let picked = $state<string[]>([]);
  let busy = $state(false);
  let error = $state<string | null>(null);

  const tools = $derived(grantedTools(connection));
  const guard = createIdentityGuard(() => connection.id);

  function nameOf(agent: ConnectionAgent): string {
    return agent.id === APOLLIA_AGENT_ID ? $t("authorizations.apollia_name") : agent.name;
  }

  $effect(() => {
    const id = connection.id;
    const ticket = guard.begin();
    holders = [];
    sharedWith = [];
    loadError = null;
    offer = null;
    picked = [];
    error = null;
    getConnectionAgents(id)
      .then((answer) => {
        if (!ticket.current) return;
        holders = answer?.agents ?? [];
        sharedWith = answer?.sharedWith ?? [];
      })
      .catch((err: unknown) => {
        if (ticket.current) loadError = messageOf(err);
      });
  });

  async function openOffer(): Promise<void> {
    const ticket = guard.begin();
    busy = true;
    error = null;
    try {
      const installed = (await listAgents()) ?? [];
      if (ticket.current) {
        offer = agentsToOffer(installed, holders, $t("authorizations.apollia_name"));
        picked = [];
      }
    } catch (err: unknown) {
      if (ticket.current) error = messageOf(err);
    } finally {
      if (ticket.current) busy = false;
    }
  }

  function toggle(id: string, on: boolean): void {
    picked = on ? [...picked, id] : picked.filter((p) => p !== id);
  }

  async function give(): Promise<void> {
    if (picked.length === 0) return;
    const ticket = guard.begin();
    busy = true;
    error = null;
    try {
      const answer = await grantConnection(connection.id, picked, tools);
      if (!ticket.current) return;
      holders = answer?.agents ?? holders;
      sharedWith = answer?.sharedWith ?? sharedWith;
      offer = null;
      picked = [];
    } catch (err: unknown) {
      if (ticket.current) error = messageOf(err);
    } finally {
      if (ticket.current) busy = false;
    }
  }
</script>

<Card class="max-w-3xl space-y-2.5 px-4 py-3.5" data-testid="connection-agents">
  <div class="flex items-center justify-between">
    <span class="text-label-md font-semibold text-foreground">{$t("connections.card.agents.title")}</span>
    <span class="text-caption text-muted-foreground">{holders.length}</span>
  </div>

  {#if loadError}
    <p class="text-caption text-destructive" data-testid="connection-agents-error">{loadError}</p>
  {:else if holders.length === 0}
    <p class="text-caption text-muted-foreground" data-testid="connection-agents-empty">
      {$t("connections.card.agents.empty")}
    </p>
  {:else}
    <ul class="space-y-1">
      {#each holders as agent (agent.id)}
        <li class="flex items-center gap-2 text-body-xs text-foreground" data-testid="connection-agent-{agent.id}">
          <Bot size={12} class="shrink-0 text-muted-foreground" aria-hidden="true" />
          <span class="min-w-0 flex-1 truncate">{nameOf(agent)}</span>
          {#if agent.byDefault}
            <Badge size="sm" variant="neutral">{$t("connections.card.agents.by_default")}</Badge>
          {/if}
          <span class="text-caption text-muted-foreground">
            {$t("connections.card.agents.tools", { values: { count: agent.tools.length } })}
          </span>
        </li>
      {/each}
    </ul>
  {/if}

  {#if sharedWith.length > 0}
    <p class="text-caption text-muted-foreground" data-testid="connection-agents-shared">
      {$t("connections.card.agents.shared")}
    </p>
  {/if}

  {#if offer === null}
    <Button
      variant="outline"
      size="sm"
      onclick={() => void openOffer()}
      loading={busy}
      disabled={busy || tools.length === 0}
      data-testid="connection-give-btn"
    >
      {#snippet icon()}<Plus size={12} />{/snippet}
      {$t("connections.card.agents.give")}
    </Button>
    {#if tools.length === 0}
      <p class="text-caption text-muted-foreground">{$t("connections.card.agents.give_no_tools")}</p>
    {/if}
  {:else}
    <div class="space-y-2 rounded-md border border-border px-3 py-2.5">
      {#if offer.length === 0}
        <p class="text-caption text-muted-foreground" data-testid="connection-give-none">
          {$t("connections.card.agents.give_none")}
        </p>
      {:else}
        <p class="text-caption text-muted-foreground">
          {$t("connections.card.agents.give_hint", { values: { count: tools.length } })}
        </p>
        <ul class="space-y-1">
          {#each offer as agent (agent.id)}
            <li class="flex items-center gap-2 text-body-xs text-foreground">
              <Checkbox
                checked={picked.includes(agent.id)}
                onchange={(on) => toggle(agent.id, on)}
                data-testid="connection-give-agent-{agent.id}"
              />
              <span>{agent.name}</span>
            </li>
          {/each}
        </ul>
      {/if}
      <div class="flex gap-2">
        {#if offer.length > 0}
          <Button
            variant="primary-solid"
            size="sm"
            onclick={() => void give()}
            loading={busy}
            disabled={busy || picked.length === 0}
            data-testid="connection-give-confirm"
          >
            {$t("connections.card.agents.give_confirm")}
          </Button>
        {/if}
        <Button variant="ghost" size="sm" onclick={() => (offer = null)} disabled={busy} data-testid="connection-give-cancel">
          {$t("common.cancel")}
        </Button>
      </div>
    </div>
  {/if}

  {#if error}
    <p class="text-caption text-destructive" data-testid="connection-give-error">{error}</p>
  {/if}
</Card>
