<script lang="ts">
  /**
   * AgentDetailHeader - the detail-pane header for a selected assistant.
   *
   * Wraps the canonical `DetailHeader`: a signature-gradient icon chip (the
   * single expressive focal point), the name, the description, the "New chat"
   * primary action, the uninstall action behind a two-step confirm, and a
   * footer row of status / version / tools / class / A2A badges.
   *
   * The uninstall used to live on `AgentCard`, deleted as an unused component
   * in April. `uninstall_agent` stayed registered and lost every caller, so an
   * installed agent became impossible to remove from the interface. The confirm
   * mirrors `PackageDetail`, with the memory option the card carried.
   */
  import { t } from "svelte-i18n";
  import { MessageSquare, Sparkles, Trash2 } from "lucide-svelte";
  import { DetailHeader, StatusDot } from "$lib/components/operator";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { Checkbox } from "$lib/components/ui/checkbox";
  import type { AgentActions } from "./useAgentActions.svelte";
  import type { AgentListItem } from "$lib/types";
  import {
    agentClassLabel,
    isActive,
    statusColor,
    statusLabel,
    statusTone,
  } from "./agentStatus";

  interface Props {
    agent: AgentListItem;
    agentActions: AgentActions;
    /**
     * Whether the uninstall confirm is armed. Owned by the route rather than
     * here, because the sidebar's context menu arms it for an agent that is
     * not the selected one yet: it selects and arms in the same gesture.
     */
    confirmingUninstall: boolean;
    onArmUninstall: () => void;
    onDisarmUninstall: () => void;
    onStartChat: (name: string) => void;
  }

  let {
    agent,
    agentActions,
    confirmingUninstall,
    onArmUninstall,
    onDisarmUninstall,
    onStartChat,
  }: Props = $props();

  const toolCount = $derived(
    agent.tools_required.length + agent.tools_optional.length,
  );

  let deleteMemory = $state(false);
  const busy = $derived(agentActions.busyKeys[`agent:${agent.name}`] === true);
  const installed = $derived(agent.installed_at !== null);

  // The memory option never survives a change of agent: it is a decision about
  // one agent's data, and carrying it across would delete something the
  // operator never chose it for.
  $effect(() => {
    void agent.name;
    deleteMemory = false;
  });

  function confirm(): void {
    const wanted = deleteMemory;
    onDisarmUninstall();
    void agentActions.uninstall(agent, wanted);
  }
</script>

<DetailHeader
  title={agent.name}
  titleTestid="agent-detail-title"
  meta={agent.description ?? $t("agents.no_description")}
>
  {#snippet leading()}
    <span
      class="inline-flex h-10 w-10 shrink-0 items-center justify-center rounded-lg bg-gradient-primary text-primary-foreground shadow-elev-2"
    >
      <Sparkles size={18} />
    </span>
  {/snippet}
  {#snippet actions()}
    {#if confirmingUninstall}
      <div class="flex flex-col items-end gap-1.5">
        <span class="text-body-xs font-medium text-destructive">
          {$t("agents.uninstall_confirm_warning", { values: { name: agent.name } })}
        </span>
        <label class="flex cursor-pointer select-none items-center gap-1.5">
          <Checkbox bind:checked={deleteMemory} data-testid="agent-uninstall-memory" />
          <span class="text-caption text-muted-foreground">
            {$t("agents.uninstall_delete_memory")}
          </span>
        </label>
        <div class="flex items-center gap-2">
          <Button variant="outline" size="sm" onclick={onDisarmUninstall}>
            {$t("common.cancel")}
          </Button>
          <Button
            variant="destructive"
            size="sm"
            disabled={busy}
            onclick={confirm}
            data-testid="agent-uninstall-confirm"
          >
            {#snippet icon()}<Trash2 size={12} />{/snippet}
            {$t("agents.uninstall_confirm_action")}
          </Button>
        </div>
      </div>
    {:else}
      {#if installed}
        <Button
          variant="outline"
          size="sm"
          onclick={onArmUninstall}
          data-testid="agent-uninstall-btn"
        >
          {#snippet icon()}<Trash2 size={12} />{/snippet}
          {$t("agents.uninstall")}
        </Button>
      {/if}
      <Button
        variant="primary-solid"
        size="sm"
        onclick={() => onStartChat(agent.name)}
        disabled={!isActive(agent)}
      >
        {#snippet icon()}<MessageSquare size={12} />{/snippet}
        {$t("agents.new_chat")}
      </Button>
    {/if}
  {/snippet}
  {#snippet footer()}
    <Badge size="sm" variant={statusTone(agent)}>
      {#snippet icon()}
        <StatusDot
          color={statusColor(agent)}
          glow={agent.runtime_status === "active"}
          size={5}
        />
      {/snippet}
      {statusLabel(agent, $t)}
    </Badge>
    <Badge size="sm" variant="neutral">v{agent.version}</Badge>
    <Badge size="sm" variant="neutral">
      {toolCount} {$t("agents.tools_word")}
    </Badge>
    {#if agentClassLabel(agent)}
      <Badge size="sm" variant="info">{agentClassLabel(agent)}</Badge>
    {/if}
    {#if agent.execution_mode}
      <Badge size="sm" variant="neutral">{agent.execution_mode}</Badge>
    {/if}
    {#if agent.supports_a2a}
      <Badge size="sm" variant="info">A2A</Badge>
    {/if}
  {/snippet}
</DetailHeader>
