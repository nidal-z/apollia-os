<script lang="ts">
  /**
   * PackageListRow - one installed package in the sidebar listbox.
   *
   * Same accessible `role="option"` shape as `AgentListRow`: a single
   * roving-tabindex main button with a right-click menu (uninstall). Shows the
   * aggregated available/total counts and a status badge. A package is never
   * started nor stopped: its agents are loaded by their first task.
   */
  import { t } from "svelte-i18n";
  import { Package, AlertTriangle, Trash2 } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { Badge } from "$lib/components/ui/badge";
  import { StatusDot } from "$lib/components/operator";
  import { agents } from "$lib/stores/agents";
  import { triggers } from "$lib/stores/sse";
  import { packageRuntimeState } from "$lib/stores/agentPackages";
  import type { ActionMenuItem } from "$lib/components/ui/action-menu";
  import type { AgentPackageListItem } from "$lib/types";
  import AgentRowContextMenu from "./AgentRowContextMenu.svelte";
  import {
    packageStatusColor,
    packageStatusLabel,
    packageStatusTone,
  } from "./agentStatus";

  interface Props {
    pkg: AgentPackageListItem;
    selected: boolean;
    onSelect: () => void;
    onUninstall: (name: string) => void;
  }

  let { pkg, selected, onSelect, onUninstall }: Props = $props();

  const state = $derived(packageRuntimeState(pkg, $agents, $triggers));

  const menuItems = $derived<ActionMenuItem[]>([
    {
      id: "uninstall",
      label: $t("agents.uninstall"),
      icon: Trash2,
      variant: "destructive",
      onclick: () => onUninstall(pkg.name),
    },
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
    data-testid="package-row-menu"
  >
    <Button
      variant="ghost"
      size="auto"
      type="button"
      data-nav-row
      onclick={onSelect}
      class="flex min-w-0 flex-1 items-center gap-2.5 rounded-lg px-2.5 py-2 text-left"
      data-testid="package-list-row"
      data-package-name={pkg.name}
    >
      <span
        class="inline-flex h-7 w-7 shrink-0 items-center justify-center rounded-lg {state.status ===
        'available'
          ? 'bg-gradient-primary text-primary-foreground shadow-sm'
          : 'border border-border bg-surface-1 text-primary'}"
      >
        <Package size={13} />
      </span>
      <span class="min-w-0 flex-1">
        <span
          class="flex items-center gap-1.5 text-body-xs {selected
            ? 'font-semibold text-foreground'
            : 'font-medium text-foreground'}"
        >
          <span class="truncate">{pkg.name}</span>
          {#if pkg.root_missing}
            <AlertTriangle size={10} class="shrink-0 text-destructive/70" />
          {:else}
            <StatusDot
              color={packageStatusColor(state)}
              glow={state.status === "available"}
              size={5}
            />
          {/if}
        </span>
        <span class="block truncate text-caption text-muted-foreground">
          {state.availableAgents}/{state.totalAgents} {$t("agents.agents_word")} · {state.enabledTriggers}/{state.totalTriggers}
          {$t("agents.triggers_word")}
        </span>
      </span>
      <Badge size="sm" variant={packageStatusTone(state)}>
        {packageStatusLabel(state, $t)}
      </Badge>
    </Button>
  </AgentRowContextMenu>
</div>
