<script lang="ts">
  /**
   * What a conversation says of its agent when the list of agents does not
   * name it: that it is being made ready while a fresh list is read, then that
   * it is no longer installed once that list still does not name it.
   *
   * The list the interface holds can lag an installation by a moment, which is
   * why it is not taken at its word (see `agentPresence`).
   */
  import { t } from "svelte-i18n";
  import { Loader2 } from "lucide-svelte";
  import { agents, refreshAgents } from "$lib/stores/sse";
  import { agentPresence } from "$lib/chat/agentPresence";
  import AgentUnavailableBanner from "./AgentUnavailableBanner.svelte";

  interface Props {
    mode: "libre" | "agent";
    agentName: string | null;
  }

  let { mode, agentName }: Props = $props();

  /** The agent a fresh list of agents was last read for. */
  let verifiedFor = $state<string | null>(null);
  const presence = $derived(
    agentPresence({
      mode,
      agentName,
      known: $agents.map((a) => a.name),
      verifiedFor,
    }),
  );

  $effect(() => {
    const name = agentName;
    if (presence !== "checking" || name === null) return;
    void refreshAgents().then((answered) => {
      // A runtime that did not answer vouches for nothing: the check stays
      // open and the next change of the list settles it.
      if (answered) verifiedFor = name;
    });
  });
</script>

{#if agentName !== null && presence === "missing"}
  <AgentUnavailableBanner {agentName} />
{:else if agentName !== null && presence === "checking"}
  <p class="flex items-center gap-2 px-4 py-2 text-caption text-muted-foreground" role="status">
    <Loader2 size={12} class="animate-spin" aria-hidden="true" />
    {$t("chat.agent_loading", { values: { agent: agentName } })}
  </p>
{/if}
