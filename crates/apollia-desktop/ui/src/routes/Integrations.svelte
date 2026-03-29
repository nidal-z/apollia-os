<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import { Plus, ArrowLeft } from "lucide-svelte";
  import { uiMode } from "$lib/stores/mode";
  import { Button } from "$lib/components/ui/button";
  import McpDisclaimerDialog, { isDisclaimerAccepted } from "../components/integrations/McpDisclaimerDialog.svelte";
  import OperatorConnectionCard from "../components/integrations/OperatorConnectionCard.svelte";
  import OperatorCatalogue from "../components/integrations/OperatorCatalogue.svelte";
  import ConnectorWizard from "../components/integrations/ConnectorWizard.svelte";
  import OperatorServerManage from "../components/integrations/OperatorServerManage.svelte";
  import type { McpServerStatusView, ConnectorEnrichmentView, RegistryServerView } from "$lib/types";
  import BuilderServerRow from "../components/integrations/BuilderServerRow.svelte";
  import BuilderServerDetail from "../components/integrations/BuilderServerDetail.svelte";
  import BuilderRegistryBrowser from "../components/integrations/BuilderRegistryBrowser.svelte";

  let disclaimerOpen = $state(false);
  let catalogueOpen = $state(false);
  let selectedServer = $state<RegistryServerView | null>(null);
  let wizardOpen = $state(false);
  let managedServerName = $state<string | null>(null);
  let manageOpen = $state(false);
  let servers = $state<McpServerStatusView[]>([]);
  let enrichmentMap = $state(new Map<string, ConnectorEnrichmentView>());
  let loading = $state(false);
  let loadError = $state<string | null>(null);

  // Builder mode state
  let builderServers = $state<McpServerStatusView[]>([]);
  let builderLoading = $state(false);
  let builderLoadError = $state<string | null>(null);
  let builderDetailServerName = $state<string | null>(null);
  let builderDetailOpen = $state(false);
  let builderRegistryOpen = $state(false);
  let builderRegistrySelected = $state<RegistryServerView | null>(null);
  let builderRegistryWizardOpen = $state(false);

  async function loadOperatorData(): Promise<void> {
    loading = true;
    loadError = null;
    try {
      const [serverList, enrichmentEntries] = await Promise.all([
        invoke<McpServerStatusView[]>("list_mcp_servers"),
        invoke<Array<{ package_identifier: string; enrichment: ConnectorEnrichmentView }>>(
          "list_mcp_enrichments",
        ),
      ]);
      servers = serverList;
      enrichmentMap = new Map(enrichmentEntries.map((e) => [e.package_identifier, e.enrichment]));
    } catch (err: unknown) {
      loadError = err instanceof Error ? err.message : String(err);
    } finally {
      loading = false;
    }
  }

  function resolveEnrichment(server: McpServerStatusView): ConnectorEnrichmentView | null {
    if (!server.package) return null;
    return enrichmentMap.get(server.package) ?? null;
  }

  function handleAddConnection(): void {
    if (isDisclaimerAccepted()) {
      catalogueOpen = true;
    } else {
      disclaimerOpen = true;
    }
  }

  function handleDisclaimerAccept(): void {
    disclaimerOpen = false;
    catalogueOpen = true;
  }

  function handleSelectServer(server: RegistryServerView): void {
    selectedServer = server;
    wizardOpen = true;
  }

  function handleWizardClose(): void {
    wizardOpen = false;
    selectedServer = null;
  }

  function handleWizardComplete(): void {
    wizardOpen = false;
    selectedServer = null;
    catalogueOpen = false;
    loadOperatorData();
  }

  function handleManage(name: string): void {
    managedServerName = name;
    manageOpen = true;
  }

  function handleManageClose(): void {
    manageOpen = false;
    managedServerName = null;
  }

  function handleManageDisconnect(): void {
    manageOpen = false;
    managedServerName = null;
    loadOperatorData();
  }

  async function loadBuilderData(): Promise<void> {
    builderLoading = true;
    builderLoadError = null;
    try {
      builderServers = await invoke<McpServerStatusView[]>("list_mcp_servers");
    } catch (err: unknown) {
      builderLoadError = err instanceof Error ? err.message : String(err);
    } finally {
      builderLoading = false;
    }
  }

  function handleBuilderRowClick(name: string): void {
    builderDetailServerName = name;
    builderDetailOpen = true;
  }

  function handleBuilderDetailClose(): void {
    builderDetailOpen = false;
    builderDetailServerName = null;
  }

  function handleBuilderRemove(): void {
    builderDetailOpen = false;
    builderDetailServerName = null;
    loadBuilderData();
  }

  function handleBuilderRegistrySelect(server: RegistryServerView): void {
    builderRegistrySelected = server;
    builderRegistryWizardOpen = true;
  }

  function handleBuilderRegistryWizardClose(): void {
    builderRegistryWizardOpen = false;
    builderRegistrySelected = null;
  }

  function handleBuilderRegistryWizardComplete(): void {
    builderRegistryWizardOpen = false;
    builderRegistrySelected = null;
    builderRegistryOpen = false;
    loadBuilderData();
  }

  $effect(() => {
    if ($uiMode === "operator") {
      loadOperatorData();
    } else {
      loadBuilderData();
    }
  });
</script>

{#if $uiMode === "operator"}
  <div class="flex flex-col gap-6" data-testid="integrations-operator">
    {#if catalogueOpen}
      <div class="flex items-center gap-3">
        <Button
          size="sm"
          variant="ghost"
          onclick={() => { catalogueOpen = false; }}
          data-testid="catalogue-back-btn"
          aria-label={$t("common.back")}
        >
          <ArrowLeft size={16} class="mr-1.5" />
          {$t("common.back")}
        </Button>
      </div>

      <OperatorCatalogue onSelectServer={handleSelectServer} />
    {:else}
      <div class="flex items-start justify-between">
        <div>
          <h1 class="text-2xl font-semibold text-foreground">{$t("nav.connections")}</h1>
          <p class="mt-1 text-sm text-muted-foreground">{$t("integrations.operator.subtitle")}</p>
        </div>
        <Button size="sm" onclick={handleAddConnection} data-testid="add-connection-btn">
          <Plus size={16} class="mr-1.5" />
          {$t("integrations.add_connection")}
        </Button>
      </div>

      {#if loading}
        <p class="text-sm text-muted-foreground" data-testid="connections-loading">
          {$t("common.loading")}
        </p>
      {:else if loadError}
        <p class="text-sm text-destructive" data-testid="connections-error">{loadError}</p>
      {:else if servers.length === 0}
        <p class="text-sm text-muted-foreground" data-testid="connections-empty">
          {$t("integrations.operator.no_connections")}
        </p>
      {:else}
        <div
          class="grid gap-4 sm:grid-cols-2 lg:grid-cols-3"
          data-testid="connections-grid"
        >
          {#each servers as server (server.name)}
            <OperatorConnectionCard
              {server}
              enrichment={resolveEnrichment(server)}
              onManage={handleManage}
            />
          {/each}
        </div>
      {/if}
    {/if}
  </div>
{:else}
  <div class="flex flex-col gap-6" data-testid="integrations-builder">
    {#if builderRegistryOpen}
      <div class="flex items-center gap-3">
        <Button
          size="sm"
          variant="ghost"
          onclick={() => { builderRegistryOpen = false; }}
          data-testid="registry-back-btn"
          aria-label={$t("common.back")}
        >
          <ArrowLeft size={16} class="mr-1.5" />
          {$t("common.back")}
        </Button>
      </div>

      <BuilderRegistryBrowser onSelectServer={handleBuilderRegistrySelect} />
    {:else}
      <div class="flex items-start justify-between">
        <div>
          <h1 class="text-2xl font-semibold text-foreground">{$t("nav.mcp_servers")}</h1>
          <p class="mt-1 text-sm text-muted-foreground">{$t("integrations.builder.subtitle")}</p>
        </div>
        <Button
          size="sm"
          onclick={() => { builderRegistryOpen = true; }}
          data-testid="browse-registry-btn"
        >
          {$t("integrations.builder.browse_registry")}
        </Button>
      </div>

      {#if builderLoading}
        <p class="text-sm text-muted-foreground" data-testid="builder-loading">
          {$t("common.loading")}
        </p>
      {:else if builderLoadError}
        <p class="text-sm text-destructive" data-testid="builder-error">{builderLoadError}</p>
      {:else if builderServers.length === 0}
        <p class="text-sm text-muted-foreground" data-testid="builder-empty">
          {$t("integrations.builder.no_servers")}
        </p>
      {:else}
        <div class="flex flex-col gap-2" data-testid="builder-servers-list">
          {#each builderServers as server (server.name)}
            <BuilderServerRow
              {server}
              onClick={() => handleBuilderRowClick(server.name)}
            />
          {/each}
        </div>
      {/if}
    {/if}
  </div>
{/if}

{#if selectedServer}
  <ConnectorWizard
    server={selectedServer}
    open={wizardOpen}
    onclose={handleWizardClose}
    oncomplete={handleWizardComplete}
  />
{/if}

{#if builderRegistrySelected}
  <ConnectorWizard
    server={builderRegistrySelected}
    open={builderRegistryWizardOpen}
    onclose={handleBuilderRegistryWizardClose}
    oncomplete={handleBuilderRegistryWizardComplete}
  />
{/if}

<McpDisclaimerDialog
  open={disclaimerOpen}
  onaccept={handleDisclaimerAccept}
  onclose={() => { disclaimerOpen = false; }}
/>

{#if builderDetailServerName !== null}
  <BuilderServerDetail
    serverName={builderDetailServerName}
    open={builderDetailOpen}
    onclose={handleBuilderDetailClose}
    onRemove={handleBuilderRemove}
  />
{/if}

{#if managedServerName !== null}
  <OperatorServerManage
    serverName={managedServerName}
    open={manageOpen}
    onclose={handleManageClose}
    onDisconnect={handleManageDisconnect}
  />
{/if}
