<script lang="ts">
  import { t } from "svelte-i18n";
  import { agents } from "$lib/stores/agents";
  import { tasks } from "$lib/stores/tasks";

  function greetingKey(): string {
    const hour = new Date().getHours();
    if (hour < 12) return "dashboard.greeting_morning";
    if (hour < 18) return "dashboard.greeting_afternoon";
    return "dashboard.greeting_evening";
  }

  let activeCount = $derived(
    $agents.filter((a) => a.runtime_status === "active" || a.runtime_status === "degraded").length,
  );

  let todayTaskCount = $derived(() => {
    const todayStart = new Date();
    todayStart.setHours(0, 0, 0, 0);
    const todayIso = todayStart.toISOString();
    return $tasks.filter((t) => t.created_at >= todayIso).length;
  });
</script>

<div
  class="relative flex items-end justify-between overflow-hidden rounded-2xl bg-gradient-surface px-5 py-5 shadow-elev-1"
  data-testid="dashboard-header"
>
  <!-- Soft primary wash — grounds the hero in the brand without overpowering. -->
  <div class="pointer-events-none absolute inset-0 bg-gradient-accent opacity-70" aria-hidden="true"></div>
  <div class="relative">
    <h1 class="text-2xl font-semibold tracking-tight" data-testid="dashboard-greeting">
      {$t(greetingKey())}
    </h1>
    <p class="mt-1 text-sm text-muted-foreground" data-testid="dashboard-subtitle">
      {$t('dashboard.subtitle')}
    </p>
  </div>
  <div class="relative flex items-center gap-1.5 rounded-full glass-surface glass-border-subtle px-3 py-1.5">
    <span class="inline-flex h-1.5 w-1.5 rounded-full bg-success"></span>
    <span class="text-xs text-muted-foreground" data-testid="dashboard-summary">
      {$t('dashboard.summary', { values: { agents: activeCount, tasks: todayTaskCount() } })}
    </span>
  </div>
</div>
