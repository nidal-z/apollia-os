<script lang="ts">
  import { t } from "svelte-i18n";
  import { currentRoute, type Route } from "$lib/stores/navigation";
  import { connectionStatus } from "$lib/stores/sse";
  import { pendingCount } from "$lib/stores/hitl";
  import { Badge } from "$lib/components/ui/badge";
  import { Separator } from "$lib/components/ui/separator";
  import {
    LayoutDashboard,
    Bot,
    ListChecks,
    ShieldCheck,
    Brain,
    Timer,
    GitBranch,
    Database,
    Bell,
    Activity,
    Settings,
    Hexagon,
  } from "lucide-svelte";
  import type { ComponentType } from "svelte";

  /** Groupe de navigation avec clé i18n et icône Lucide. */
  type NavItem = { route: Route; labelKey: string; icon: ComponentType };
  type NavGroup = { labelKey: string; items: NavItem[] };

  const navGroups: NavGroup[] = [
    {
      labelKey: "nav.operations",
      items: [
        { route: "dashboard", labelKey: "nav.dashboard", icon: LayoutDashboard },
        { route: "agents", labelKey: "nav.agents", icon: Bot },
        { route: "tasks", labelKey: "nav.tasks", icon: ListChecks },
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

  const CONNECTION_KEYS: Record<string, string> = {
    connecting: "nav.connection.connecting",
    connected: "nav.connection.connected",
    reconnecting: "nav.connection.reconnecting",
    error: "nav.connection.error",
  };
</script>

<aside class="flex h-screen w-60 flex-col border-r bg-card" data-testid="sidebar">
  <!-- Logo -->
  <div class="flex items-center gap-2.5 px-4 py-5">
    <Hexagon size={22} class="text-primary" />
    <span class="text-xl font-bold text-primary" data-testid="sidebar-logo">Apollia OS</span>
  </div>

  <Separator />

  <!-- Navigation groups -->
  <nav class="flex flex-1 flex-col p-3" data-testid="sidebar-nav">
    {#each navGroups as group, groupIndex}
      <span class="mb-1 mt-3 px-3 text-[11px] font-semibold uppercase tracking-wider text-muted-foreground/60" data-testid="nav-group-{group.labelKey.split('.')[1]}"
        >{$t(group.labelKey)}</span
      >
      {#each group.items as item}
        <button
          class="flex w-full items-center gap-3 rounded-md px-3 py-2 text-sm font-medium transition-colors {$currentRoute ===
          item.route
            ? 'bg-accent text-accent-foreground'
            : 'text-muted-foreground hover:bg-accent/50 hover:text-accent-foreground'}"
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
      {#if groupIndex < navGroups.length - 1}
        <Separator class="my-2" />
      {/if}
    {/each}

    <!-- Spacer to push settings to bottom -->
    <div class="flex-1"></div>

    <Separator class="my-2" />

    <!-- Settings (bottom, before connection indicator) -->
    <button
      class="flex w-full items-center gap-3 rounded-md px-3 py-2 text-sm font-medium transition-colors {$currentRoute ===
      settingsItem.route
        ? 'bg-accent text-accent-foreground'
        : 'text-muted-foreground hover:bg-accent/50 hover:text-accent-foreground'}"
      data-testid="nav-{settingsItem.route}"
      onclick={() => navigate(settingsItem.route)}
    >
      <settingsItem.icon size={18} />
      <span>{$t(settingsItem.labelKey)}</span>
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
