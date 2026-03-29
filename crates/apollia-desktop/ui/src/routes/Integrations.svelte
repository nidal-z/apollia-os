<script lang="ts">
  import { t } from "svelte-i18n";
  import { uiMode } from "$lib/stores/mode";
</script>

{#if $uiMode === "operator"}
  <!-- Operator view: MCP connections -->
  <div class="flex flex-col gap-6" data-testid="integrations-operator">
    <div>
      <h1 class="text-2xl font-semibold text-foreground">{$t("nav.connections")}</h1>
      <p class="mt-1 text-sm text-muted-foreground">{$t("integrations.operator.subtitle")}</p>
    </div>
  </div>
{:else}
  <!-- Builder view: MCP Servers -->
  <div class="flex flex-col gap-6" data-testid="integrations-builder">
    <div>
      <h1 class="text-2xl font-semibold text-foreground">{$t("nav.mcp_servers")}</h1>
      <p class="mt-1 text-sm text-muted-foreground">{$t("integrations.builder.subtitle")}</p>
    </div>
  </div>
{/if}
