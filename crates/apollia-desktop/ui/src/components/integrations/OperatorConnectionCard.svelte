<script lang="ts">
  import { t } from "svelte-i18n";
  import {
    Database,
    Search,
    Folder,
    Brain,
    Globe,
    Plug2,
  } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { formatRelativeTime } from "$lib/utils";
  import type { McpServerStatusView, ConnectorEnrichmentView } from "$lib/types";
  import TrustBadge from "./TrustBadge.svelte";
  import ConnectionStatusIndicator from "./ConnectionStatusIndicator.svelte";

  // All lucide-svelte icons share the same component shape — typed via typeof Database.
  const ICON_MAP: Record<string, typeof Database> = {
    database: Database,
    search: Search,
    folder: Folder,
    brain: Brain,
    globe: Globe,
  };

  interface Props {
    server: McpServerStatusView;
    enrichment: ConnectorEnrichmentView | null;
    onManage: (name: string) => void;
  }

  let { server, enrichment, onManage }: Props = $props();

  const title = $derived(enrichment?.operator_label ?? server.name);
  const Icon: typeof Database = $derived(
    enrichment ? (ICON_MAP[enrichment.icon_name] ?? Plug2) : Plug2,
  );
  const lastActivity = $derived(
    server.last_call_at ? formatRelativeTime(server.last_call_at) : null,
  );
  const statusBarColor = $derived(
    server.connected ? "bg-emerald-500" : server.error ? "bg-red-500" : "bg-muted",
  );
</script>

<div
  class="glass-card glass-border rounded-xl overflow-hidden flex flex-col"
  data-testid="operator-connection-card"
  data-server-name={server.name}
>
  <!-- Status accent bar -->
  <div class="h-0.5 w-full {statusBarColor}" aria-hidden="true"></div>

  <div class="px-4 pt-3.5 pb-3 flex flex-col gap-3 flex-1">
    <!-- Row 1: icon + title + trust badge -->
    <div class="flex items-start gap-3">
      <div
        class="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-muted text-muted-foreground"
        aria-hidden="true"
      >
        <Icon size={18} />
      </div>
      <div class="min-w-0 flex-1">
        <div class="flex items-center gap-2 flex-wrap">
          <span
            class="text-[13px] font-medium text-foreground truncate"
            data-testid="connection-card-title"
          >
            {title}
          </span>
          {#if enrichment}
            <TrustBadge level={enrichment.trust_level} />
          {/if}
        </div>
        <div class="mt-0.5">
          <ConnectionStatusIndicator connected={server.connected} error={server.error} />
        </div>
      </div>
    </div>

    <!-- Row 2: tools count + last activity -->
    <div class="flex items-center justify-between text-xs text-muted-foreground">
      <span data-testid="connection-card-tools-count">
        {$t("integrations.operator.tools_active", { values: { count: server.tools_count } })}
      </span>
      {#if lastActivity !== null}
        <span data-testid="connection-card-last-activity">{lastActivity}</span>
      {:else}
        <span class="text-muted-foreground/50" data-testid="connection-card-never">
          {$t("integrations.operator.never")}
        </span>
      {/if}
    </div>

    <!-- Row 3: manage button -->
    <div class="flex justify-end">
      <Button
        size="sm"
        variant="outline"
        onclick={() => onManage(server.name)}
        data-testid="connection-card-manage-btn"
      >
        {$t("integrations.operator.manage")}
      </Button>
    </div>
  </div>
</div>
