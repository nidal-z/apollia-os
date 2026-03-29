<script lang="ts">
  import { t } from "svelte-i18n";
  import { Plus } from "lucide-svelte";
  import { uiMode } from "$lib/stores/mode";
  import { Button } from "$lib/components/ui/button";
  import McpDisclaimerDialog, { isDisclaimerAccepted } from "../components/integrations/McpDisclaimerDialog.svelte";

  let disclaimerOpen = $state(false);

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
