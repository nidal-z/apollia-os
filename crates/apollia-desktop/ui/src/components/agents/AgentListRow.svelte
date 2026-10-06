<script lang="ts">
  /**
   * AgentListRow - one agent in the Agents column of the screen.
   *
   * Rendered as an accessible `role="option"` with a single roving-tabindex
   * main button (the primary control), which opens the sheet; right-click /
   * the context-menu key opens the row's actions, and "Chat" starts a
   * conversation with the agent. There is no start nor stop: an agent is
   * loaded by its first task and released once idle.
   */
  import { t } from "svelte-i18n";
  import { MessageSquare, Sparkles, ScrollText, Settings, Trash2 } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { StatusDot } from "$lib/components/operator";
  import type { ActionMenuItem } from "$lib/components/ui/action-menu";
  import type { AgentListItem } from "$lib/types";
  import AgentRowContextMenu from "./AgentRowContextMenu.svelte";
  import { availability, statusColor, statusLabel } from "./agentStatus";

  interface Props {
    agent: AgentListItem;
    selected: boolean;
    onSelect: () => void;
    onOpenLogs: (agentId: string) => void;
    onOpenSettings: (agent: AgentListItem) => void;
    onRequestUninstall: (agent: AgentListItem) => void;
    /** Starts a conversation with the agent. */
    onStartChat: (name: string) => void;
  }

  let {
    agent,
    selected,
    onSelect,
    onOpenLogs,
    onOpenSettings,
    onRequestUninstall,
    onStartChat,
  }: Props = $props();

  const available = $derived(availability(agent) !== "unavailable");

  const menuItems = $derived<ActionMenuItem[]>([
    ...(agent.id
      ? [
          {
            id: "logs",
            label: $t("agents.logs"),
            icon: ScrollText,
            onclick: () => onOpenLogs(agent.id as string),
          },
        ]
      : []),
    {
      id: "settings",
      label: $t("agents.tab_settings"),
      icon: Settings,
      onclick: () => onOpenSettings(agent),
    },
    // Select first, then arm the confirm in the detail header: a right-click
    // that deletes an agent outright, without ever showing what is about to
    // go, is not a gesture to offer for an irreversible action.
    ...(agent.installed_at !== null
      ? [
          {
            id: "uninstall",
            label: $t("agents.uninstall"),
            icon: Trash2,
            variant: "destructive" as const,
            onclick: () => onRequestUninstall(agent),
          },
        ]
      : []),
  ]);
</script>

<div
  role="option"
  aria-selected={selected}
  class="group mb-0.5 flex w-full items-center gap-1 rounded-lg pr-1 transition-colors {selected
    ? 'bg-primary/10'
    : 'hover:bg-surface-1'}"
>
  <AgentRowContextMenu
    items={menuItems}
    class="min-w-0 flex-1"
    data-testid="agent-row-menu"
  >
    <Button
      variant="ghost"
      size="auto"
      type="button"
      data-nav-row
      onclick={onSelect}
      class="flex min-w-0 flex-1 items-center gap-2.5 rounded-lg px-2.5 py-2 text-left"
      data-testid="agent-list-row"
      data-agent-name={agent.name}
    >
      <span
        class="inline-flex h-7 w-7 shrink-0 items-center justify-center rounded-lg {available
          ? 'bg-gradient-primary text-primary-foreground shadow-sm'
          : 'border border-border bg-surface-1 text-primary'}"
      >
        <Sparkles size={13} />
      </span>
      <span class="min-w-0 flex-1">
        <span
          class="flex items-center gap-1.5 text-body-xs {selected
            ? 'font-semibold text-foreground'
            : 'font-medium text-foreground'}"
        >
          <span class="truncate">{agent.name}</span>
          <StatusDot color={statusColor(agent)} glow={available} size={5} />
        </span>
        <span class="block truncate text-caption text-muted-foreground">
          {agent.description ?? statusLabel(agent, $t)}
        </span>
      </span>
    </Button>
  </AgentRowContextMenu>
  {#if available}
    <Button
      variant="outline"
      size="sm"
      class="shrink-0"
      onclick={() => onStartChat(agent.name)}
      data-testid="agent-row-chat"
    >
      {#snippet icon()}<MessageSquare size={11} />{/snippet}
      {$t("agents.row_chat")}
    </Button>
  {/if}
</div>
