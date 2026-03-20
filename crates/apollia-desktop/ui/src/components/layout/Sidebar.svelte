<script lang="ts">
  import { t } from "svelte-i18n";
  import { currentRoute, type Route } from "$lib/stores/navigation";
  import { connectionStatus } from "$lib/stores/sse";
  import { pendingCount } from "$lib/stores/hitl";
  import { uiMode } from "$lib/stores/mode";
  import { Badge } from "$lib/components/ui/badge";
  import { Separator } from "$lib/components/ui/separator";
  import {
    LayoutDashboard,
    Bot,
    ListChecks,
    MessageSquare,
    ShieldCheck,
    Brain,
    Timer,
    GitBranch,
    Database,
    Bell,
    Activity,
    Settings,
    Layers,
  } from "lucide-svelte";
  import type { ComponentType } from "svelte";

  type NavItem = { route: Route; labelKey: string; icon: ComponentType };
  type NavGroup = { labelKey: string; items: NavItem[] };

  /** Flat list shown in operator mode (no group headers). */
  const operatorNav: NavItem[] = [
    { route: "dashboard", labelKey: "nav.home", icon: LayoutDashboard },
    { route: "agents", labelKey: "nav.my_assistants", icon: Bot },
    { route: "tasks", labelKey: "nav.activity", icon: ListChecks },
    { route: "chat", labelKey: "nav.chat", icon: MessageSquare },
    { route: "approvals", labelKey: "nav.approvals", icon: ShieldCheck },
  ];

  /** Grouped list shown in builder mode. */
  const builderNavGroups: NavGroup[] = [
    {
      labelKey: "nav.operations",
      items: [
        { route: "dashboard", labelKey: "nav.dashboard", icon: LayoutDashboard },
        { route: "agents", labelKey: "nav.agents", icon: Bot },
        { route: "tasks", labelKey: "nav.tasks", icon: ListChecks },
        { route: "chat", labelKey: "nav.chat", icon: MessageSquare },
        { route: "approvals", labelKey: "nav.approvals", icon: ShieldCheck },
      ],
    },
    {
      labelKey: "nav.infrastructure",
      items: [
        { route: "llm", labelKey: "nav.llm", icon: Brain },
        { route: "triggers", labelKey: "nav.triggers", icon: Timer },
        { route: "pipelines", labelKey: "nav.pipelines", icon: GitBranch },
      ],
    },
    {
      labelKey: "nav.data",
      items: [
        { route: "memory", labelKey: "nav.memory", icon: Database },
        { route: "notifications", labelKey: "nav.notifications", icon: Bell },
        { route: "observability", labelKey: "nav.observability", icon: Activity },
      ],
    },
  ];

  const settingsItem: NavItem = {
    route: "settings",
    labelKey: "nav.settings",
    icon: Settings,
  };

  function navigate(route: Route) {
    currentRoute.set(route);
  }

  function toggleMode() {
    uiMode.update((m) => (m === "operator" ? "builder" : "operator"));
  }

  const CONNECTION_KEYS: Record<string, string> = {
    connecting: "nav.connection.connecting",
    connected: "nav.connection.connected",
    reconnecting: "nav.connection.reconnecting",
    error: "nav.connection.error",
  };

  const isOperator = $derived($uiMode === "operator");
</script>

<aside class="flex h-screen w-60 flex-col glass-panel border-r border-[rgba(52,53,245,0.08)] dark:border-[rgba(124,95,214,0.10)]" data-testid="sidebar">
  <!-- Logo -->
  <div class="flex items-center gap-2.5 px-4 py-5">
    <img src="/logo.svg" alt="Apollia" width="32" height="32" class="h-8 w-8" />
    <span class="text-lg font-bold bg-gradient-to-r from-apollia-blue to-apollia-violet bg-clip-text text-transparent" data-testid="sidebar-logo">Apollia OS</span>
  </div>

  <Separator />

  <!-- Navigation -->
  <nav class="flex flex-1 flex-col p-3" data-testid="sidebar-nav">
    {#if isOperator}
      <!-- Operator mode: flat list, no group headers -->
      {#each operatorNav as item}
        <button
          class="flex w-full items-center gap-3 rounded-md px-3 py-2 text-sm font-medium transition-colors {$currentRoute === item.route
            ? 'bg-primary/10 text-primary'
            : 'text-muted-foreground hover:bg-primary/5 hover:text-foreground'}"
          data-testid="nav-{item.route}"
          onclick={() => navigate(item.route)}
        >
          <item.icon size={18} />
          <span>{$t(item.labelKey)}</span>
          {#if item.route === "approvals" && $pendingCount > 0}
            <Badge variant="destructive" class="ml-auto animate-pulse text-[10px] px-1.5 py-0" data-testid="approvals-badge"
              >{$pendingCount}</Badge
            >
          {/if}
        </button>
      {/each}
    {:else}
      <!-- Builder mode: grouped nav with section headers -->
      {#each builderNavGroups as group, groupIndex}
        <span
          class="mb-1 mt-3 px-3 text-[11px] font-semibold uppercase tracking-wider text-muted-foreground/60"
          data-testid="nav-group-{group.labelKey.split('.')[1]}"
          >{$t(group.labelKey)}</span
        >
        {#each group.items as item}
          <button
            class="flex w-full items-center gap-3 rounded-md px-3 py-2 text-sm font-medium transition-colors {$currentRoute === item.route
              ? 'bg-primary/10 text-primary'
              : 'text-muted-foreground hover:bg-primary/5 hover:text-foreground'}"
            data-testid="nav-{item.route}"
            onclick={() => navigate(item.route)}
          >
            <item.icon size={18} />
            <span>{$t(item.labelKey)}</span>
            {#if item.route === "approvals" && $pendingCount > 0}
              <Badge variant="destructive" class="ml-auto animate-pulse text-[10px] px-1.5 py-0" data-testid="approvals-badge"
                >{$pendingCount}</Badge
              >
            {/if}
          </button>
        {/each}
        {#if groupIndex < builderNavGroups.length - 1}
          <Separator class="my-2" />
        {/if}
      {/each}
    {/if}

    <!-- Spacer -->
    <div class="flex-1"></div>

    <Separator class="my-2" />

    <!-- Settings -->
    <button
      class="flex w-full items-center gap-3 rounded-md px-3 py-2 text-sm font-medium transition-colors {$currentRoute === settingsItem.route
        ? 'bg-primary/10 text-primary'
        : 'text-muted-foreground hover:bg-primary/5 hover:text-foreground'}"
      data-testid="nav-{settingsItem.route}"
      onclick={() => navigate(settingsItem.route)}
    >
      <settingsItem.icon size={18} />
      <span>{$t(settingsItem.labelKey)}</span>
    </button>

    <!-- Mode toggle -->
    <button
      class="mt-1 flex w-full items-center gap-3 rounded-md px-3 py-2 text-sm font-medium text-muted-foreground transition-colors hover:bg-primary/5 hover:text-foreground"
      data-testid="mode-toggle"
      onclick={toggleMode}
      title={isOperator ? $t("nav.switch_to_builder") : $t("nav.switch_to_operator")}
    >
      <Layers size={18} />
      <span class="text-xs">{isOperator ? $t("nav.switch_to_builder") : $t("nav.switch_to_operator")}</span>
    </button>
  </nav>

  <Separator />

  <!-- Connection indicator -->
  <div class="flex items-center gap-2 px-4 py-3" data-testid="connection-status" data-status={$connectionStatus}>
    {#if $connectionStatus === "connected"}
      <span class="h-2 w-2 rounded-full bg-[var(--apollia-success)]" data-testid="connection-dot"></span>
    {:else if $connectionStatus === "reconnecting"}
      <span class="h-2 w-2 animate-pulse rounded-full bg-[var(--apollia-warning)]" data-testid="connection-dot"></span>
    {:else}
      <span class="h-2 w-2 rounded-full bg-[hsl(var(--destructive))]" data-testid="connection-dot"></span>
    {/if}
    <span class="text-xs text-muted-foreground" data-testid="connection-label"
      >{$t(CONNECTION_KEYS[$connectionStatus] ?? 'common.unknown')}</span
    >
  </div>
</aside>
