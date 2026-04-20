<script lang="ts" module>
  import type { McpServerStatusView } from "$lib/types";

  export type HealthLevel = "healthy" | "degraded" | "error";

  export function resolveHealth(server: McpServerStatusView): HealthLevel {
    if (server.error) return "error";
    if (!server.connected) return "degraded";
    return "healthy";
  }
</script>

<script lang="ts">
  import { t } from "svelte-i18n";

  interface Props {
    server: McpServerStatusView;
    onclick?: () => void;
  }

  let { server, onclick }: Props = $props();

  const level = $derived<HealthLevel>(resolveHealth(server));

  const DOT_CLASS: Record<HealthLevel, string> = {
    healthy: "bg-emerald-500",
    degraded: "bg-amber-500",
    error: "bg-red-500 animate-pulse",
  };

  const LABEL_KEY: Record<HealthLevel, string> = {
    healthy: "connections.health.healthy",
    degraded: "connections.health.degraded",
    error: "connections.health.error",
  };

  const tooltip = $derived(
    level === "degraded" || level === "error"
      ? server.error ?? $t(LABEL_KEY[level])
      : $t(LABEL_KEY.healthy),
  );
</script>

<button
  type="button"
  class="inline-flex items-center gap-1.5 rounded-md px-1.5 py-0.5 text-xs text-muted-foreground transition-colors hover:bg-muted/60 focus:outline-none focus-visible:ring-2 focus-visible:ring-primary/50"
  {onclick}
  title={tooltip}
  data-testid="connection-health-indicator"
  data-health={level}
  aria-label={tooltip}
>
  <span class="inline-block h-2 w-2 rounded-full {DOT_CLASS[level]}" aria-hidden="true"></span>
  <span>{$t(LABEL_KEY[level])}</span>
</button>
