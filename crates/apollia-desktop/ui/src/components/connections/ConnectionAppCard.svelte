<script lang="ts" module>
  export type CardState = "not_installed" | "installed" | "error" | "degraded";
</script>

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
  import { TrustBadge } from "$lib/components/shared";
  import type {
    McpServerStatusView,
    ConnectorEnrichmentView,
    RegistryServerView,
    TrustLevel,
  } from "$lib/types";
  import ConnectionHealthIndicator from "./ConnectionHealthIndicator.svelte";

  const ICON_MAP: Record<string, typeof Database> = {
    database: Database,
    search: Search,
    folder: Folder,
    brain: Brain,
    globe: Globe,
  };

  interface Props {
    /** Installed server (present when the connection is already configured). */
    server?: McpServerStatusView | null;
    /** Registry entry (present when the connection is discoverable but not installed). */
    registry?: RegistryServerView | null;
    /** Enrichment shared by both registry + installed views. */
    enrichment: ConnectorEnrichmentView | null;
    /** Number of tools — `tools_count` for installed, may come from registry enrichment for catalogue. */
    toolsCount: number;
    installed: boolean;
    onconnect?: (registry: RegistryServerView) => void;
    onmanage?: (serverName: string) => void;
    onreconnect?: (serverName: string) => void;
    onhealthClick?: (server: McpServerStatusView) => void;
  }

  let {
    server = null,
    registry = null,
    enrichment,
    toolsCount,
    installed,
    onconnect,
    onmanage,
    onreconnect,
    onhealthClick,
  }: Props = $props();

  const title = $derived(
    enrichment?.operator_label ??
      registry?.title ??
      registry?.name ??
      server?.name ??
      "",
  );

  const description = $derived(
    registry?.description ?? enrichment?.auth_help_text ?? null,
  );

  const Icon: typeof Database = $derived(
    enrichment ? (ICON_MAP[enrichment.icon_name] ?? Plug2) : Plug2,
  );

  const trustLevel = $derived<TrustLevel | null>(
    enrichment?.trust_level ?? registry?.trust_level ?? null,
  );

  const state = $derived<CardState>(
    !installed
      ? "not_installed"
      : server?.error
        ? "error"
        : server?.connected === false
          ? "degraded"
          : "installed",
  );

  const cardClass = $derived(
    state === "installed"
      ? "glass-card glass-border"
      : state === "error"
        ? "glass-card border-destructive/30"
        : "glass-card glass-border hover:border-primary/40 hover:shadow-md",
  );

  function handleCta() {
    if (state === "not_installed" && registry) {
      onconnect?.(registry);
      return;
    }
    if (state === "error" && server) {
      onreconnect?.(server.name);
      return;
    }
    if (server) {
      onmanage?.(server.name);
    }
  }

  const ctaLabel = $derived(
    state === "not_installed"
      ? $t("connections.card.cta_connect")
      : state === "error"
        ? $t("connections.card.cta_reconnect")
        : $t("connections.card.cta_manage"),
  );

  const ctaVariant: "default" | "outline" | "destructive" = $derived(
    state === "not_installed" ? "default" : state === "error" ? "destructive" : "outline",
  );
</script>

<div
  class="rounded-xl overflow-hidden flex flex-col transition-all duration-150 {cardClass}"
  data-testid="connection-app-card"
  data-state={state}
  data-name={server?.name ?? registry?.name ?? ""}
>
  <div class="px-4 pt-4 pb-3 flex flex-col gap-3 flex-1">
    <!-- Row 1: icon + title + trust -->
    <div class="flex items-start gap-3">
      <div
        class="flex h-12 w-12 shrink-0 items-center justify-center rounded-lg bg-muted text-muted-foreground"
        aria-hidden="true"
      >
        <Icon size={22} />
      </div>
      <div class="min-w-0 flex-1">
        <div class="flex items-start justify-between gap-2">
          <span class="text-sm font-semibold text-foreground truncate" data-testid="connection-card-title">
            {title}
          </span>
          {#if installed}
            <span
              class="shrink-0 inline-flex items-center rounded-full bg-emerald-500/15 px-2 py-0.5 text-[10px] font-semibold text-emerald-700 dark:text-emerald-400"
              data-testid="connection-card-installed-badge"
            >
              {$t("connections.card.installed")}
            </span>
          {/if}
        </div>
        <div class="mt-0.5 flex items-center gap-2">
          {#if trustLevel !== null}
            <TrustBadge level={trustLevel} />
          {/if}
        </div>
      </div>
    </div>

    <!-- Description -->
    {#if description}
      <p class="text-xs text-muted-foreground line-clamp-2" data-testid="connection-card-description">
        {description}
      </p>
    {/if}

    <!-- Footer: tools count + health + CTA -->
    <div class="mt-auto flex items-center justify-between gap-2 pt-2">
      <div class="flex items-center gap-2 text-xs text-muted-foreground min-w-0">
        <span data-testid="connection-card-tools-count">
          {$t("connections.card.tools_available", { values: { count: toolsCount } })}
        </span>
        {#if installed && server}
          <span aria-hidden="true">·</span>
          <ConnectionHealthIndicator
            {server}
            onclick={() => onhealthClick?.(server)}
          />
        {/if}
      </div>

      <Button
        size="sm"
        variant={ctaVariant}
        onclick={handleCta}
        data-testid="connection-card-cta"
        data-action={state}
      >
        {ctaLabel}
      </Button>
    </div>
  </div>
</div>
