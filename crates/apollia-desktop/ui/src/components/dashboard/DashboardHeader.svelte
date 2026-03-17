<script lang="ts">
  import { t } from "svelte-i18n";
  import { agents } from "$lib/stores/agents";
  import { tasks } from "$lib/stores/tasks";

  /** Return a time-of-day i18n key based on current hour. */
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

<div class="space-y-1" data-testid="dashboard-header">
  <h1 class="text-2xl font-bold" data-testid="dashboard-greeting">
    {$t(greetingKey())}
  </h1>
  <p class="text-sm text-muted-foreground" data-testid="dashboard-subtitle">
    {$t('dashboard.subtitle')}
  </p>
  <p class="text-xs text-muted-foreground" data-testid="dashboard-summary">
    {$t('dashboard.summary', { values: { agents: activeCount, tasks: todayTaskCount() } })}
  </p>
</div>
