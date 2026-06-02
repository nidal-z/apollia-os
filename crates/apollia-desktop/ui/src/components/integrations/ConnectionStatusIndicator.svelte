<script module lang="ts">
  import type { McpHealth } from "$lib/types";

  export type StatusConfig = { dot: string; labelKey: string };

  /**
   * Map operational health to a dot colour utility and an i18n label key.
   *
   * Health is the single source of truth: `connected` (process alive) is not
   * enough, a session can be alive yet degraded or needing re-authorization.
   */
  export function statusConfigForHealth(health: McpHealth): StatusConfig {
    switch (health.state) {
      case "healthy":
        return {
          dot: "bg-success",
          labelKey: health.verified
            ? "integrations.status.connected"
            : "integrations.status.unverified",
        };
      case "degraded":
        return { dot: "bg-warning", labelKey: "integrations.status.degraded" };
      case "needs_reauth":
        return { dot: "bg-warning", labelKey: "integrations.status.needs_reauth" };
      case "unavailable":
        return { dot: "bg-destructive", labelKey: "integrations.status.unavailable" };
    }
  }
</script>

<script lang="ts">
  import { t } from "svelte-i18n";

  interface Props {
    /** Operational health driving the dot colour and label. */
    health: McpHealth;
  }

  let { health }: Props = $props();

  const status = $derived(statusConfigForHealth(health));
</script>

<span class="inline-flex items-center gap-1.5">
  <span class="inline-block h-2 w-2 rounded-full {status.dot}" aria-hidden="true"></span>
  <span class="text-xs text-muted-foreground">{$t(status.labelKey)}</span>
</span>
