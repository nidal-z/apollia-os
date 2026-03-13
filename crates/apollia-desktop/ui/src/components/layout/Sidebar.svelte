<script lang="ts">
  import { currentRoute, type Route } from "$lib/stores/navigation";
  import { connectionStatus } from "$lib/stores/sse";
  import { pendingCount } from "$lib/stores/hitl";
  import { Badge } from "$lib/components/ui/badge";
  import { Separator } from "$lib/components/ui/separator";

  /** Groupe de navigation avec label et items. */
  type NavGroup = {
    label: string;
    items: { route: Route; label: string; icon: string }[];
  };

  const navGroups: NavGroup[] = [
    {
      label: "Operations",
      items: [
        { route: "agents", label: "Agents", icon: "🤖" },
        { route: "tasks", label: "Tasks", icon: "📋" },
        { route: "approvals", label: "Approvals", icon: "✋" },
      ],
    },
    {
      label: "Infrastructure",
      items: [
        { route: "llm", label: "LLM", icon: "🧠" },
        { route: "triggers", label: "Triggers", icon: "⏱️" },
        { route: "pipelines", label: "Pipelines", icon: "🔗" },
      ],
    },
    {
      label: "Data",
      items: [
        { route: "memory", label: "Memory", icon: "💾" },
        { route: "notifications", label: "Notifications", icon: "🔔" },
        { route: "observability", label: "Observability", icon: "📊" },
      ],
    },
  ];

  const settingsItem: { route: Route; label: string; icon: string } = {
    route: "settings",
    label: "Settings",
    icon: "⚙️",
  };

  function navigate(route: Route) {
    currentRoute.set(route);
  }

  const STATUS_LABELS: Record<string, string> = {
    connecting: "Connecting...",
    connected: "Runtime connected",
    reconnecting: "Reconnecting...",
    error: "Connection lost",
  };
</script>

<aside class="flex h-screen w-60 flex-col border-r bg-card" data-testid="sidebar">
  <!-- Logo -->
  <div class="flex items-center gap-2 px-4 py-5">
    <span class="text-xl font-bold text-primary" data-testid="sidebar-logo">Apollia OS</span>
  </div>

  <Separator />

  <!-- Navigation groups -->
  <nav class="flex flex-1 flex-col p-3" data-testid="sidebar-nav">
    {#each navGroups as group, groupIndex}
      <span class="mb-1 mt-3 px-3 text-[11px] font-semibold uppercase tracking-wider text-muted-foreground/60" data-testid="nav-group-{group.label.toLowerCase()}"
        >{group.label}</span
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
          <span>{item.icon}</span>
          <span>{item.label}</span>
          {#if item.route === "approvals" && $pendingCount > 0}
            <Badge variant="destructive" class="ml-auto text-[10px] px-1.5 py-0" data-testid="approvals-badge"
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
      <span>{settingsItem.icon}</span>
      <span>{settingsItem.label}</span>
    </button>
  </nav>

  <Separator />

  <!-- Connection indicator -->
  <div class="flex items-center gap-2 px-4 py-3" data-testid="connection-status" data-status={$connectionStatus}>
    {#if $connectionStatus === "connected"}
      <span class="h-2 w-2 rounded-full bg-[var(--apollia-success)]" data-testid="connection-dot"></span>
    {:else if $connectionStatus === "reconnecting"}
      <span class="h-2 w-2 rounded-full bg-[var(--apollia-warning)]" data-testid="connection-dot"></span>
    {:else}
      <span class="h-2 w-2 rounded-full bg-[hsl(var(--destructive))]" data-testid="connection-dot"></span>
    {/if}
    <span class="text-xs text-muted-foreground" data-testid="connection-label"
      >{STATUS_LABELS[$connectionStatus] ?? "Unknown"}</span
    >
  </div>
</aside>
