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
  import { navigateTo } from "$lib/stores/navigation";

  interface Props {
    /** Missing agent name — surfaced in the banner copy. */
    agentName: string;
  }

  let { agentName }: Props = $props();
</script>

<div
  class="flex items-center justify-between gap-3 border-b border-warning/30
    bg-warning/10 px-4 py-2 text-[12px] text-warning-foreground"
  role="status"
  aria-live="polite"
  data-testid="agent-unavailable-banner"
>
  <div class="flex items-center gap-2">
    <AlertTriangle size={14} aria-hidden="true" />
    <span>
      {$t("chat.agent_unavailable.message", { values: { agent: agentName } })}
    </span>
  </div>
  <button
    type="button"
    class="rounded px-2 py-0.5 text-[11px] font-medium underline-offset-2
      hover:underline focus-visible:outline-none focus-visible:ring-2
      focus-visible:ring-primary/40"
    onclick={() => navigateTo("agents")}
    data-testid="agent-unavailable-install"
  >
    {$t("chat.agent_unavailable.manage")}
  </button>
</div>
