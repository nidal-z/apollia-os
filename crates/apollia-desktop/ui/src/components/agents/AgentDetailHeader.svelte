<script lang="ts">
  /**
   * AgentDetailHeader - the detail-pane header for a selected assistant.
   *
   * Wraps the canonical `DetailHeader`: a signature-gradient icon chip (the
   * single expressive focal point), the name, the description, the "New chat"
   * primary action, and a footer row of status / version / tools / class /
   * A2A badges.
   */
  import { t } from "svelte-i18n";
  import { MessageSquare, Sparkles } from "lucide-svelte";
  import { DetailHeader, StatusDot } from "$lib/components/operator";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
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
    onStartChat: (name: string) => void;
  }

  let { agent, onStartChat }: Props = $props();

  const toolCount = $derived(
    agent.tools_required.length + agent.tools_optional.length,
  );
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
    <Button
      variant="primary-solid"
      size="sm"
      onclick={() => onStartChat(agent.name)}
      disabled={!isActive(agent)}
    >
      {#snippet icon()}<MessageSquare size={12} />{/snippet}
      {$t("agents.new_chat")}
    </Button>
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
