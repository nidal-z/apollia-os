<script lang="ts">
  /**
   * AgentUnavailableBanner — inline notice shown when the agent that
   * owns the current session has been uninstalled or is no longer in
   * the agents store.
   *
   * Purely informative — the conversation keeps rendering so history
   * stays accessible; new messages will surface the runtime's own error.
   */
  import { t } from "svelte-i18n";
  import { AlertTriangle } from "lucide-svelte";
  import { Banner } from "$lib/components/ui/banner";
  import { navigateTo } from "$lib/stores/navigation";
  import { Button } from "$lib/components/ui/button";

  interface Props {
    /** Missing agent name — surfaced in the banner copy. */
    agentName: string;
  }

  let { agentName }: Props = $props();
</script>

<Banner
  variant="warning"
  icon={AlertTriangle}
  data-testid="agent-unavailable-banner"
>
  {$t("chat.agent_unavailable.message", { values: { agent: agentName } })}

  {#snippet trailing()}
    <Button variant="ghost" size="sm"
      type="button"
      class="rounded px-2 py-0.5 text-[11px] font-medium underline-offset-2
        hover:underline focus-visible:outline-none focus-visible:ring-2
        focus-visible:ring-primary/40"
      onclick={() => navigateTo("agents")}
      data-testid="agent-unavailable-install"
    >
      {$t("chat.agent_unavailable.manage")}
    </Button>
  {/snippet}
</Banner>
