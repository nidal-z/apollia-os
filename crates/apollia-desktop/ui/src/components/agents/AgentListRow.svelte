<script lang="ts">
  /**
   * AgentListRow - one assistant in the sidebar listbox.
   *
   * Rendered as an accessible `role="option"` with a single roving-tabindex
   * main button (the primary control) plus a hover-revealed start/stop button;
   * right-click / the context-menu key opens the same actions. This replaces
   * the former whole-card-clickable div (which suppressed a11y lints).
   */
  import { t } from "svelte-i18n";
  import { Sparkles, Play, Square, ScrollText, Settings, Trash2 } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { Spinner } from "$lib/components/ui/progress";
  import { StatusDot } from "$lib/components/operator";
  import type { ActionMenuItem } from "$lib/components/ui/action-menu";
  import type { AgentListItem } from "$lib/types";
  import AgentRowContextMenu from "./AgentRowContextMenu.svelte";
  import type { AgentActions } from "./useAgentActions.svelte";
  import { isActive, isIdle, statusColor, statusLabel } from "./agentStatus";

  interface Props {
    agent: AgentListItem;
    selected: boolean;
    actions: AgentActions;
    onSelect: () => void;
    onOpenLogs: (agentId: string) => void;
    onOpenSettings: (agent: AgentListItem) => void;
    onRequestUninstall: (agent: AgentListItem) => void;
  }

  let {
    agent,
    selected,
    actions,
    onSelect,
    onOpenLogs,
    onOpenSettings,
    onRequestUninstall,
  }: Props = $props();

  const running = $derived(isActive(agent));
  const transitioning = $derived(
    agent.runtime_status === "initializing" ||
      agent.runtime_status === "stopping",
  );
  const busy = $derived(
    actions.busyKeys[`agent:${agent.name}`] === true || transitioning,
  );

  const menuItems = $derived<ActionMenuItem[]>([
    {
      id: "toggle",
      label: running ? $t("agents.stop") : $t("agents.start"),
      icon: running ? Square : Play,
      onclick: () => actions.toggleAgentRuntime(agent),
      disabled: busy || (!running && !agent.install_path),
    },
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
        class="inline-flex h-7 w-7 shrink-0 items-center justify-center rounded-lg {running
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
          {#if running}
            <StatusDot color={statusColor(agent)} glow size={5} />
          {:else if isIdle(agent)}
            <StatusDot color={statusColor(agent)} size={5} />
          {/if}
        </span>
        <span class="block truncate text-caption text-muted-foreground">
          {agent.description ?? statusLabel(agent, $t)}
        </span>
      </span>
    </Button>
  </AgentRowContextMenu>

  <Button
    variant="ghost"
    size="icon-sm"
    type="button"
    tabindex={-1}
    onclick={() => actions.toggleAgentRuntime(agent)}
    disabled={busy || (!running && !agent.install_path)}
    class="-ml-1 text-muted-foreground transition-opacity hover:bg-transparent hover:text-foreground disabled:opacity-40 {running ||
    busy
      ? 'opacity-100'
      : 'opacity-0 focus-visible:opacity-100 group-hover:opacity-100'}"
    aria-label={running
      ? $t("agents.stop_named", { values: { name: agent.name } })
      : $t("agents.start_named", { values: { name: agent.name } })}
    title={running ? $t("agents.stop") : $t("agents.start")}
    data-testid="agent-list-toggle"
    data-agent-name={agent.name}
  >
    {#if busy}
      <Spinner size={13} />
    {:else if running}
      <Square size={12} fill="currentColor" />
    {:else}
      <Play size={13} fill="currentColor" />
    {/if}
  </Button>
</div>
