<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import { RefreshCw } from "lucide-svelte";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { Search } from "lucide-svelte";
  import type { RegistryServerView } from "$lib/types";

  const REGISTRY_SOURCE = "registry.modelcontextprotocol.io";

  interface Props {
    onSelectServer: (server: RegistryServerView) => void;
  }

  let { onSelectServer }: Props = $props();

  let servers = $state<RegistryServerView[]>([]);
  let loading = $state(false);
  let loadError = $state<string | null>(null);
  let syncing = $state(false);
  let searchQuery = $state("");
  let lastSyncTime = $state<string | null>(null);

  /** Servers visible after applying the search filter. */
  const filteredServers = $derived.by(() => {
    const query = searchQuery.trim().toLowerCase();
    if (query.length === 0) return servers;
    return servers.filter((s) => {
      const inName = s.name.toLowerCase().includes(query);
      const inDescription = (s.description ?? "").toLowerCase().includes(query);
      return inName || inDescription;
    });
  });

  async function loadRegistry(): Promise<void> {
    loading = true;
    loadError = null;
    try {
      servers = await invoke<RegistryServerView[]>("fetch_mcp_registry");
      lastSyncTime = new Date().toLocaleString();
    } catch (err: unknown) {
      loadError = err instanceof Error ? err.message : String(err);
    } finally {
      loading = false;
    }
  }

  async function syncNow(): Promise<void> {
    syncing = true;
    loadError = null;
    try {
      servers = await invoke<RegistryServerView[]>("fetch_mcp_registry");
      lastSyncTime = new Date().toLocaleString();
    } catch (err: unknown) {
      loadError = err instanceof Error ? err.message : String(err);
    } finally {
      syncing = false;
    }
  }

  $effect(() => {
    loadRegistry();
  });
</script>

<div class="flex flex-col gap-4" data-testid="builder-registry-browser">
  <!-- Header -->
  <div class="flex items-start justify-between gap-4">
    <div>
      <h2 class="text-lg font-semibold text-foreground">
        {$t("integrations.builder.registry.title")}
      </h2>
      <p class="mt-0.5 text-sm text-muted-foreground">
        {$t("integrations.builder.registry.subtitle")}
      </p>
    </div>
    <Button
      size="sm"
      variant="outline"
      onclick={syncNow}
      disabled={syncing || loading}
      data-testid="registry-sync-btn"
    >
      <RefreshCw
        size={14}
        class="mr-1.5 {syncing ? 'animate-spin' : ''}"
        aria-hidden="true"
      />
      {syncing
        ? $t("integrations.builder.registry.syncing")
        : $t("integrations.builder.registry.sync_button")}
    </Button>
  </div>

  <!-- Search -->
  {#if !loading && !loadError}
    <div class="relative" data-testid="registry-search-bar">
      <Search
        size={15}
        class="absolute left-3 top-1/2 -translate-y-1/2 text-muted-foreground pointer-events-none"
        aria-hidden="true"
      />
      <Input
        type="search"
        class="pl-9"
        value={searchQuery}
        placeholder={$t("integrations.builder.registry.search_placeholder")}
        oninput={(e) => { searchQuery = (e.currentTarget as HTMLInputElement).value; }}
        data-testid="registry-search-input"
        aria-label={$t("integrations.builder.registry.search_placeholder")}
      />
    </div>
  {/if}

  <!-- Body -->
  {#if loading}
    <p class="text-sm text-muted-foreground" data-testid="registry-loading">
      {$t("common.loading")}
    </p>
  {:else if loadError}
    <p class="text-sm text-destructive" data-testid="registry-error">{loadError}</p>
  {:else if filteredServers.length === 0}
    <p
      class="py-6 text-center text-sm text-muted-foreground"
      data-testid="registry-no-results"
    >
      {$t("integrations.builder.registry.no_results")}
    </p>
  {:else}
    <div class="flex flex-col gap-1" data-testid="registry-server-list">
      {#each filteredServers as server (server.name)}
        {@const firstPkg = server.packages?.[0] ?? null}
        <button
          type="button"
          class="glass-card glass-border w-full rounded-lg px-4 py-3 text-left transition-colors hover:bg-muted/30"
          onclick={() => onSelectServer(server)}
          data-testid="registry-server-row"
          data-server-name={server.name}
        >
          <div class="flex flex-wrap items-center gap-x-3 gap-y-1">
            <!-- Technical name -->
            <span
              class="font-mono text-sm font-medium text-foreground"
              data-testid="registry-row-name"
            >
              {server.name}
            </span>

            <!-- Version -->
            <span
              class="shrink-0 rounded bg-muted px-1.5 py-0.5 font-mono text-xs text-muted-foreground"
              data-testid="registry-row-version"
            >
              v{server.version}
            </span>

            <!-- Transport type from first package -->
            {#if firstPkg}
              <Badge variant="secondary" data-testid="registry-row-transport">
                {firstPkg.transport_type}
              </Badge>
            {/if}

            <!-- Env var count from first package -->
            {#if firstPkg && firstPkg.environment_variables.length > 0}
              <span
                class="shrink-0 text-xs text-muted-foreground"
                data-testid="registry-row-env-count"
              >
                {$t("integrations.builder.registry.env_count", {
                  values: { count: firstPkg.environment_variables.length },
                })}
              </span>
            {/if}
          </div>

          <!-- Description -->
          {#if server.description}
            <p
              class="mt-1 line-clamp-1 text-xs text-muted-foreground"
              data-testid="registry-row-description"
            >
              {server.description}
            </p>
          {/if}

          <!-- Package identifier -->
          {#if firstPkg}
            <p
              class="mt-0.5 font-mono text-xs text-muted-foreground/70"
              data-testid="registry-row-identifier"
            >
              {firstPkg.identifier}
            </p>
          {/if}
        </button>
      {/each}
    </div>
  {/if}

  <!-- Footer -->
  <div
    class="mt-2 flex flex-wrap items-center gap-x-4 gap-y-1 border-t border-border pt-3 text-xs text-muted-foreground"
    data-testid="registry-footer"
  >
    <span data-testid="registry-footer-source">
      {$t("integrations.builder.registry.source_label")}:
      <span class="font-mono">{REGISTRY_SOURCE}</span>
    </span>
    <span data-testid="registry-footer-sync-time">
      {$t("integrations.builder.registry.last_sync_label")}:
      {lastSyncTime ?? $t("integrations.builder.registry.never")}
    </span>
  </div>
</div>
