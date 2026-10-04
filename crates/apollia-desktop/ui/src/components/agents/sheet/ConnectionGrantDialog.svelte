<script lang="ts">
  /**
   * ConnectionGrantDialog - grants the agent the tools of one connection.
   * The connection is chosen first; its tools are then listed in the five
   * action families, plus "unclassified" when one has no family, and start
   * checked as the runtime offers them: only the reading tools of a
   * connection from the selection, or an official one. A connection that
   * sends data off this machine carries the "Remote" badge. Nothing is
   * written until the sheet is saved.
   */
  import { t } from "svelte-i18n";
  import { Plug } from "lucide-svelte";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { Checkbox } from "$lib/components/ui/checkbox";
  import { Dialog, DialogFooter } from "$lib/components/ui/dialog";
  import type { Connection } from "$lib/types";
  import { ACCESS_GROUP_LABEL, ACCESS_GROUP_RULE } from "./agentSheet";
  import {
    CONNECTION_PROVENANCE_LABEL,
    connectionTitle,
    connectionToolGroups,
    initialPicks,
    type ConnectionOffer,
  } from "./connectionGrant";

  interface Props {
    open: boolean;
    /** Name of the agent the connection is granted to. */
    agentName: string;
    /** The accesses of the draft. */
    granted: string[];
    loadOffer: () => Promise<ConnectionOffer>;
    onclose: () => void;
    onconfirm: (ids: string[]) => void;
  }

  let { open, agentName, granted, loadOffer, onclose, onconfirm }: Props = $props();

  let offer = $state<ConnectionOffer>({ connections: [], descriptions: new Map() });
  let loading = $state(false);
  let chosen = $state<Connection | null>(null);
  let picked = $state<string[]>([]);

  $effect(() => {
    if (!open) return;
    chosen = null;
    picked = [];
    loading = true;
    let live = true;
    void loadOffer()
      .then((value) => {
        if (live) offer = value;
      })
      .catch(() => {
        if (live) offer = { connections: [], descriptions: new Map() };
      })
      .finally(() => {
        if (live) loading = false;
      });
    return () => {
      live = false;
    };
  });

  const groups = $derived(chosen ? connectionToolGroups(chosen, offer.descriptions, granted) : []);

  function choose(connection: Connection): void {
    chosen = connection;
    picked = initialPicks(connection, granted);
  }

  function toggle(id: string, on: boolean): void {
    picked = on ? [...picked, id] : picked.filter((p) => p !== id);
  }
</script>

{#snippet card(connection: Connection)}
  <span class="flex size-8 shrink-0 items-center justify-center rounded-md bg-muted text-muted-foreground">
    <Plug size={14} />
  </span>
  <span class="min-w-0 flex-1">
    <span class="block text-body-xs text-foreground">{connectionTitle(connection)}</span>
    <span class="block text-caption text-muted-foreground">
      {connection.official
        ? $t("agent_adjust.connection_provenance_official", {
            values: { provenance: $t(CONNECTION_PROVENANCE_LABEL[connection.provenance]) },
          })
        : $t(CONNECTION_PROVENANCE_LABEL[connection.provenance])}
    </span>
  </span>
  {#if connection.remote}
    <Badge size="sm" variant="info" data-testid="agent-adjust-connection-remote">{$t("agent_sheet.remote")}</Badge>
  {/if}
{/snippet}

<Dialog
  {open}
  {onclose}
  size="md"
  title={chosen
    ? $t("agent_adjust.connection_title", { values: { agent: agentName } })
    : $t("agent_adjust.connection_choose_title")}
  data-testid="agent-adjust-connection-dialog"
>
  {#if loading}
    <p class="m-0 text-body-xs text-muted-foreground">{$t("common.loading")}</p>
  {:else if chosen === null}
    <p class="m-0 mb-3 text-caption text-muted-foreground">{$t("agent_adjust.connection_choose_legend")}</p>
    {#if offer.connections.length === 0}
      <p class="m-0 text-body-xs italic text-muted-foreground" data-testid="agent-adjust-connection-none">
        {$t("agent_adjust.connection_none")}
      </p>
    {:else}
      <ul class="m-0 max-h-80 list-none space-y-2 overflow-y-auto p-0">
        {#each offer.connections as connection (connection.id)}
          <li>
            <button
              type="button"
              class="flex w-full items-center gap-3 rounded-md border border-border p-3 text-left hover:bg-muted/50"
              onclick={() => choose(connection)}
              data-testid="agent-adjust-connection-choice"
              data-connection={connection.id}
            >
              {@render card(connection)}
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  {:else}
    <p class="m-0 mb-3 text-caption text-muted-foreground">{$t("agent_adjust.connection_legend")}</p>
    <div
      class="flex items-center gap-3 rounded-md border border-border p-3"
      data-testid="agent-adjust-connection-card"
      data-connection={chosen.id}
    >
      {@render card(chosen)}
    </div>
    <div class="mt-3 max-h-80 space-y-3 overflow-y-auto">
      {#each groups as group (group.key)}
        <div data-testid="agent-adjust-connection-family" data-family={group.key}>
          <div class="flex items-baseline gap-2">
            <span class="text-body-xs font-semibold text-foreground">{$t(ACCESS_GROUP_LABEL[group.key])}</span>
            <span class="text-caption text-muted-foreground">{$t(ACCESS_GROUP_RULE[group.key])}</span>
          </div>
          {#if group.tools.length === 0}
            <p class="m-0 mt-1 text-caption italic text-muted-foreground">
              {$t("agent_adjust.connection_none_in_family")}
            </p>
          {:else}
            <ul class="m-0 mt-1 list-none space-y-1 p-0">
              {#each group.tools as tool (tool.id)}
                <li class="flex items-start gap-2 py-1" data-testid="agent-adjust-connection-tool" data-access={tool.id}>
                  <Checkbox
                    checked={tool.granted || picked.includes(tool.id)}
                    disabled={tool.granted}
                    onchange={(on) => toggle(tool.id, on)}
                    data-testid="agent-adjust-connection-check"
                  />
                  <div class="min-w-0 flex-1">
                    <div class="text-body-xs text-foreground">{tool.name}</div>
                    {#if tool.granted || tool.description}
                      <div class="text-caption text-muted-foreground">
                        {tool.granted ? $t("agent_adjust.grant_already") : tool.description}
                      </div>
                    {/if}
                  </div>
                </li>
              {/each}
            </ul>
          {/if}
        </div>
      {/each}
    </div>
  {/if}
  <DialogFooter>
    {#if chosen}
      <Button variant="ghost" onclick={() => (chosen = null)} data-testid="agent-adjust-connection-back">
        {$t("agent_adjust.connection_back")}
      </Button>
    {/if}
    <Button variant="outline" onclick={onclose} data-testid="agent-adjust-connection-cancel">{$t("common.cancel")}</Button>
    {#if chosen}
      <Button
        variant="primary-solid"
        disabled={picked.length === 0}
        onclick={() => onconfirm(picked)}
        data-testid="agent-adjust-connection-confirm"
      >
        {$t("agent_adjust.connection_confirm", { values: { count: picked.length } })}
      </Button>
    {/if}
  </DialogFooter>
</Dialog>
