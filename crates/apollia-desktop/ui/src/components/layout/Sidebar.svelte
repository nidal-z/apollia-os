<script lang="ts">
  import { t } from "svelte-i18n";
  import { currentRoute, navigateTo, sidebarCollapsed, type Route } from "$lib/stores/navigation";
  import { connectionStatus } from "$lib/stores/sse";
  import { pendingCount } from "$lib/stores/hitl";
  import { activeChatCount } from "$lib/stores/chat";
  import { uiMode } from "$lib/stores/mode";
  import { Badge } from "$lib/components/ui/badge";
  import { Separator } from "$lib/components/ui/separator";
  import { showOnboardingBadge } from "$lib/stores/onboarding";
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
    PanelLeftClose,
    PanelLeftOpen,
    Sparkles,
  } from "lucide-svelte";
  import type { ComponentType } from "svelte";

  type NavItem = { route: Route; labelKey: string; icon: ComponentType };
  type NavGroup = { labelKey: string; items: NavItem[] };

  const operatorNav: NavItem[] = [
    { route: "dashboard", labelKey: "nav.home", icon: LayoutDashboard },
    { route: "agents", labelKey: "nav.my_assistants", icon: Bot },
    { route: "tasks", labelKey: "nav.activity", icon: ListChecks },
    { route: "chat", labelKey: "nav.chat", icon: MessageSquare },
    { route: "approvals", labelKey: "nav.approvals", icon: ShieldCheck },
  ];

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
    navigateTo(route);
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
  const collapsed = $derived($sidebarCollapsed);
</script>

<aside
  class="flex h-screen flex-col glass-panel border-r glass-border transition-[width] duration-200 ease-apple"
  class:w-60={!collapsed}
  class:w-16={collapsed}
  data-testid="sidebar"
>
  <!-- Logo + Collapse toggle -->
  <div class="flex items-center gap-2.5 px-4 py-5" class:justify-center={collapsed}>
    <img src="/logo.svg" alt="Apollia" width="32" height="32" class="h-8 w-8 shrink-0" />
    {#if !collapsed}
      <span class="text-lg font-semibold text-foreground transition-opacity duration-150" data-testid="sidebar-logo">Apollia OS</span>
      <button
        class="ml-auto rounded-md p-1 text-muted-foreground hover:bg-muted hover:text-foreground transition-colors"
        onclick={() => sidebarCollapsed.toggle()}
        title={$t("nav.collapse_sidebar")}
        data-testid="sidebar-collapse-btn"
      >
        <PanelLeftClose size={16} strokeWidth={1.75} />
      </button>
    {:else}
      <button
        class="absolute left-4 top-5 rounded-md p-1 text-muted-foreground hover:bg-muted hover:text-foreground transition-colors"
        onclick={() => sidebarCollapsed.toggle()}
        title={$t("nav.expand_sidebar")}
        data-testid="sidebar-expand-btn"
        style="display: none;"
      >
        <PanelLeftOpen size={16} strokeWidth={1.75} />
      </button>
    {/if}
  </div>

  {#if collapsed}
    <!-- Expand button visible in collapsed mode -->
    <button
      class="mx-auto mb-2 rounded-md p-1.5 text-muted-foreground hover:bg-muted hover:text-foreground transition-colors"
      onclick={() => sidebarCollapsed.toggle()}
      title={$t("nav.expand_sidebar")}
      data-testid="sidebar-expand-btn"
    >
      <PanelLeftOpen size={16} strokeWidth={1.75} />
    </button>
  {/if}

  <Separator />

  <!-- Navigation -->
  <nav class="flex flex-1 flex-col p-2" class:p-3={!collapsed} data-testid="sidebar-nav">
    {#if isOperator}
      {#each operatorNav as item}
        {@const isActive = $currentRoute === item.route}
        <button
          class="flex w-full items-center gap-3 rounded-md px-3 py-2 text-sm font-medium transition-colors {isActive
            ? 'bg-primary/10 text-primary'
            : 'text-muted-foreground hover:bg-primary/[0.04] hover:text-foreground'}"
          class:justify-center={collapsed}
          class:px-2={collapsed}
          data-testid="nav-{item.route}"
          onclick={() => navigate(item.route)}
          title={collapsed ? $t(item.labelKey) : undefined}
        >
          <item.icon size={18} strokeWidth={1.75} class="shrink-0" />
          {#if !collapsed}
            <span>{$t(item.labelKey)}</span>
            {#if item.route === "approvals" && $pendingCount > 0}
              <Badge variant="destructive" class="ml-auto text-[10px] px-1.5 py-0" data-testid="approvals-badge"
                >{$pendingCount}</Badge
              >
            {/if}
            {#if item.route === "chat" && $activeChatCount > 0}
              <Badge variant="secondary" class="ml-auto text-[10px] px-1.5 py-0" data-testid="chat-badge"
                >{$activeChatCount}</Badge
              >
            {/if}
          {/if}
        </button>
      {/each}
    {:else}
      {#each builderNavGroups as group, groupIndex}
        {#if !collapsed}
          <span
            class="mb-1 mt-3 px-3 text-[11px] font-semibold uppercase tracking-wider text-muted-foreground/60"
            data-testid="nav-group-{group.labelKey.split('.')[1]}"
            >{$t(group.labelKey)}</span
          >
        {:else if groupIndex > 0}
          <Separator class="my-1.5" />
        {/if}
        {#each group.items as item}
          {@const isActive = $currentRoute === item.route}
          <button
            class="flex w-full items-center gap-3 rounded-md px-3 py-2 text-sm font-medium transition-colors {isActive
              ? 'bg-primary/10 text-primary'
              : 'text-muted-foreground hover:bg-primary/[0.04] hover:text-foreground'}"
            class:justify-center={collapsed}
            class:px-2={collapsed}
            data-testid="nav-{item.route}"
            onclick={() => navigate(item.route)}
            title={collapsed ? $t(item.labelKey) : undefined}
          >
            <item.icon size={18} strokeWidth={1.75} class="shrink-0" />
            {#if !collapsed}
              <span>{$t(item.labelKey)}</span>
              {#if item.route === "approvals" && $pendingCount > 0}
                <Badge variant="destructive" class="ml-auto text-[10px] px-1.5 py-0" data-testid="approvals-badge"
                  >{$pendingCount}</Badge
                >
              {/if}
              {#if item.route === "chat" && $activeChatCount > 0}
                <Badge variant="secondary" class="ml-auto text-[10px] px-1.5 py-0" data-testid="chat-badge"
                  >{$activeChatCount}</Badge
                >
              {/if}
            {/if}
          </button>
        {/each}
        {#if !collapsed && groupIndex < builderNavGroups.length - 1}
          <Separator class="my-2" />
        {/if}
      {/each}
    {/if}

    <div class="flex-1"></div>

    <Separator class="my-2" />

    <!-- Settings -->
    <button
      class="flex w-full items-center gap-3 rounded-md px-3 py-2 text-sm font-medium transition-colors {$currentRoute === settingsItem.route
        ? 'bg-primary/10 text-primary'
        : 'text-muted-foreground hover:bg-primary/[0.04] hover:text-foreground'}"
      class:justify-center={collapsed}
      class:px-2={collapsed}
      data-testid="nav-{settingsItem.route}"
      onclick={() => navigate(settingsItem.route)}
      title={collapsed ? $t(settingsItem.labelKey) : undefined}
    >
      <settingsItem.icon size={18} strokeWidth={1.75} class="shrink-0" />
      {#if !collapsed}
        <span>{$t(settingsItem.labelKey)}</span>
      {/if}
    </button>

    <!-- Onboarding badge -->
    {#if $showOnboardingBadge}
      <button
        class="flex w-full items-center gap-3 rounded-md px-3 py-2 text-sm font-medium text-secondary transition-colors hover:bg-secondary/10"
        class:justify-center={collapsed}
        class:px-2={collapsed}
        data-testid="onboarding-badge"
        onclick={() => navigate("onboarding")}
        title={collapsed ? $t("onboarding_welcome.badge") : undefined}
      >
        <Sparkles size={18} strokeWidth={1.75} class="shrink-0" />
        {#if !collapsed}
          <span>{$t("onboarding_welcome.badge")}</span>
          <Badge variant="secondary" class="ml-auto text-[10px] px-1.5 py-0">!</Badge>
        {/if}
      </button>
    {/if}

    <!-- Mode toggle -->
    {#if !collapsed}
      <button
        class="mt-1 flex w-full items-center gap-3 rounded-md px-3 py-2 text-sm font-medium text-muted-foreground transition-colors hover:bg-primary/[0.04] hover:text-foreground"
        data-testid="mode-toggle"
        onclick={toggleMode}
        title={isOperator ? $t("nav.switch_to_builder") : $t("nav.switch_to_operator")}
      >
        <Layers size={18} strokeWidth={1.75} />
        <span class="text-xs">{isOperator ? $t("nav.switch_to_builder") : $t("nav.switch_to_operator")}</span>
      </button>
    {:else}
      <button
        class="mt-1 flex w-full items-center justify-center rounded-md px-2 py-2 text-sm font-medium text-muted-foreground transition-colors hover:bg-primary/[0.04] hover:text-foreground"
        data-testid="mode-toggle"
        onclick={toggleMode}
        title={isOperator ? $t("nav.switch_to_builder") : $t("nav.switch_to_operator")}
      >
        <Layers size={18} strokeWidth={1.75} />
      </button>
    {/if}
  </nav>

  <Separator />

  <!-- Connection indicator -->
  <div class="flex items-center gap-2 px-4 py-3" class:justify-center={collapsed} class:px-2={collapsed} data-testid="connection-status" data-status={$connectionStatus}>
    {#if $connectionStatus === "connected"}
      <span class="h-2 w-2 shrink-0 rounded-full bg-success" data-testid="connection-dot"></span>
    {:else if $connectionStatus === "reconnecting"}
      <span class="h-2 w-2 shrink-0 animate-pulse rounded-full bg-warning" data-testid="connection-dot"></span>
    {:else}
      <span class="h-2 w-2 shrink-0 rounded-full bg-destructive" data-testid="connection-dot"></span>
    {/if}
    {#if !collapsed}
      <span class="text-xs text-muted-foreground" data-testid="connection-label"
        >{$t(CONNECTION_KEYS[$connectionStatus] ?? 'common.unknown')}</span
      >
    {/if}
  </div>
</aside>
