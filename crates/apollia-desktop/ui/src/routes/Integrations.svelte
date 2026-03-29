<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import { Plus } from "lucide-svelte";
  import { uiMode } from "$lib/stores/mode";
  import { Button } from "$lib/components/ui/button";
  import McpDisclaimerDialog, { isDisclaimerAccepted } from "../components/integrations/McpDisclaimerDialog.svelte";
  import OperatorConnectionCard from "../components/integrations/OperatorConnectionCard.svelte";
  import type { McpServerStatusView, ConnectorEnrichmentView } from "$lib/types";

  let disclaimerOpen = $state(false);
  let servers = $state<McpServerStatusView[]>([]);
  let enrichmentMap = $state(new Map<string, ConnectorEnrichmentView>());
  let loading = $state(false);
  let loadError = $state<string | null>(null);

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
      // ConnectorWizard will be wired here in STORY-357
    } else {
      disclaimerOpen = true;
    }
  }

  function handleDisclaimerAccept(): void {
    disclaimerOpen = false;
    // ConnectorWizard will be wired here in STORY-357
  }

  function handleManage(_name: string): void {
    // OperatorServerManage sheet will be wired here in STORY-358
  }

  $effect(() => {
    if ($uiMode === "operator") {
      loadOperatorData();
    }
  });
</script>

{#if $uiMode === "operator"}
  <div class="flex flex-col gap-6" data-testid="integrations-operator">
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
  </div>
{:else}
  <div class="flex flex-col gap-6" data-testid="integrations-builder">
    <div>
      <h1 class="text-2xl font-semibold text-foreground">{$t("nav.mcp_servers")}</h1>
      <p class="mt-1 text-sm text-muted-foreground">{$t("integrations.builder.subtitle")}</p>
    </div>
  </div>
{/if}

<McpDisclaimerDialog
  open={disclaimerOpen}
  onaccept={handleDisclaimerAccept}
  onclose={() => { disclaimerOpen = false; }}
/>
