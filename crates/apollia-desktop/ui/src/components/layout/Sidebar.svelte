<script lang="ts">
  import { currentRoute, type Route } from "$lib/stores/navigation";
  import { connectionStatus } from "$lib/stores/sse";
  import { pendingCount } from "$lib/stores/hitl";
  import { Badge } from "$lib/components/ui/badge";
  import { Separator } from "$lib/components/ui/separator";

  const navItems: { route: Route; label: string; icon: string }[] = [
    { route: "agents", label: "Agents", icon: "🤖" },
    { route: "tasks", label: "Tasks", icon: "📋" },
    { route: "approvals", label: "Approvals", icon: "✋" },
    { route: "llm", label: "LLM", icon: "🧠" },
    { route: "triggers", label: "Triggers", icon: "⏱️" },
    { route: "pipelines", label: "Pipelines", icon: "🔗" },
    { route: "memory", label: "Memory", icon: "💾" },
  ];

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

<aside class="flex h-screen w-60 flex-col border-r bg-card">
  <!-- Logo -->
  <div class="flex items-center gap-2 px-4 py-5">
    <span class="text-xl font-bold text-primary">Apollia OS</span>
  </div>

  <Separator />

  <!-- Navigation -->
  <nav class="flex flex-1 flex-col gap-1 p-3">
    {#each navItems as item}
      <button
        class="flex w-full items-center gap-3 rounded-md px-3 py-2 text-sm font-medium transition-colors {$currentRoute ===
        item.route
          ? 'bg-accent text-accent-foreground'
          : 'text-muted-foreground hover:bg-accent/50 hover:text-accent-foreground'}"
        onclick={() => navigate(item.route)}
      >
        <span>{item.icon}</span>
        <span>{item.label}</span>
        {#if item.route === "approvals" && $pendingCount > 0}
          <Badge variant="destructive" class="ml-auto text-[10px] px-1.5 py-0"
            >{$pendingCount}</Badge
          >
        {/if}
      </button>
    {/each}
  </nav>

  <Separator />

  <!-- Connection indicator -->
  <div class="flex items-center gap-2 px-4 py-3">
    {#if $connectionStatus === "connected"}
      <span class="h-2 w-2 rounded-full bg-[var(--apollia-success)]"></span>
    {:else if $connectionStatus === "reconnecting"}
      <span class="h-2 w-2 rounded-full bg-[var(--apollia-warning)]"></span>
    {:else}
      <span class="h-2 w-2 rounded-full bg-[hsl(var(--destructive))]"></span>
    {/if}
    <span class="text-xs text-muted-foreground"
      >{STATUS_LABELS[$connectionStatus] ?? "Unknown"}</span
    >
  </div>
</aside>
