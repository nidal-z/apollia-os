<script lang="ts">
  /**
   * The agents and workflows the local server serves like models: one card
   * per exposed item, with the identifier a client calls it by, its kind and
   * what a call takes. With none exposed, the section says where the flag is
   * set.
   */
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import { Copy } from "lucide-svelte";
  import { Banner } from "$lib/components/ui/banner";
  import { Button } from "$lib/components/ui/button";
  import { addToast } from "$lib/components/ui/toast";
  import { developerMode } from "$lib/stores/developerMode";
  import { openSettingsWindow, settingsWindowOpen } from "$lib/stores/settingsWindow";
  import { listMcpExposable } from "$lib/ipc/mcpServer";
  import { getWorkflow } from "$lib/ipc/workflows";
  import ExplainedBadge from "../ai/ExplainedBadge.svelte";
  import ServedCard from "../ai/ServedCard.svelte";
  import { openBridgeSettings } from "./developerScreen";
  import {
    SERVED_PREFIX,
    copyIdentifier,
    readServedItems,
    servedItemSlug,
    type ServedInputs,
    type ServedItem,
    type ServedItemsIpc,
  } from "./servedItems";

  const ipc: ServedItemsIpc = { list: listMcpExposable, workflow: getWorkflow };

  let items = $state<ServedItem[] | null>(null);
  let failed = $state(false);

  async function read(): Promise<void> {
    try {
      items = await readServedItems(ipc);
      failed = false;
    } catch {
      failed = true;
    }
  }

  onMount(() => {
    void read();
    // The flag is set in the settings: read again once they close.
    let wasOpen = false;
    return settingsWindowOpen.subscribe((open) => {
      if (wasOpen && !open) void read();
      wasOpen = open;
    });
  });

  function inputsText(inputs: ServedInputs): string {
    if (inputs.kind === "message") return $t("dev.developer.exposed.inputs_message");
    if (inputs.kind === "unknown") return $t("dev.developer.exposed.inputs_unknown");
    if (inputs.names.length === 0) return $t("dev.developer.exposed.inputs_none");
    return $t("dev.developer.exposed.inputs_fields", {
      values: { names: inputs.names.join(", ") },
    });
  }

  async function copy(item: ServedItem): Promise<void> {
    const done = await copyIdentifier(item.identifier, (text) =>
      navigator.clipboard.writeText(text),
    );
    addToast(
      $t(done ? "dev.developer.models.copied" : "dev.developer.models.copy_failed"),
      done ? "success" : "error",
    );
  }
</script>

{#if $developerMode}
  <section class="space-y-3" data-testid="developer-exposed-section">
    <div class="flex items-baseline gap-2">
      <h3 class="text-sm font-semibold text-foreground">{$t("dev.developer.exposed.title")}</h3>
      {#if items}
        <span class="text-xs text-muted-foreground" data-testid="developer-exposed-count">
          {items.length}
        </span>
      {/if}
    </div>

    {#if failed}
      <Banner variant="destructive" surface="card" data-testid="developer-exposed-error">
        {$t("dev.developer.exposed.error")}
      </Banner>
    {:else if items && items.length === 0}
      <div
        class="space-y-2 rounded-lg border border-border bg-card p-3"
        data-testid="developer-exposed-empty"
      >
        <p class="text-sm font-medium text-foreground">
          {$t("dev.developer.exposed.empty_title")}
        </p>
        <p class="text-xs text-muted-foreground">
          {$t("dev.developer.exposed.empty_body", {
            values: { identifier: `${SERVED_PREFIX}<id>` },
          })}
        </p>
        <Button
          variant="outline"
          size="sm"
          onclick={() => openBridgeSettings(openSettingsWindow)}
          data-testid="developer-exposed-open-settings"
        >
          {$t("dev.developer.exposed.empty_action")}
        </Button>
      </div>
    {:else if items}
      <div class="space-y-2">
        {#each items as item (item.identifier)}
          <ServedCard testid={`developer-exposed-card-${servedItemSlug(item)}`}>
            {#snippet badges()}
              <ExplainedBadge
                label={$t("dev.developer.exposed.badge")}
                explanation={$t("dev.developer.exposed.badge_explanation")}
                variant="success"
                testid="developer-exposed-badge"
              />
              <ExplainedBadge
                label={item.kind}
                explanation={$t(
                  item.kind === "workflow"
                    ? "dev.developer.exposed.workflow_explanation"
                    : "dev.developer.exposed.agent_explanation",
                )}
                help
                testid="developer-exposed-kind"
              />
            {/snippet}
            {#snippet identity()}
              <span
                class="truncate font-mono text-sm font-medium text-foreground"
                data-testid="developer-exposed-identifier"
              >
                {item.identifier}
              </span>
            {/snippet}
            {#snippet facts()}
              <span class="text-sm text-foreground" data-testid="developer-exposed-inputs">
                {inputsText(item.inputs)}
              </span>
              <span class="text-sm text-muted-foreground">
                {$t("dev.developer.exposed.governed")}
              </span>
            {/snippet}
            {#snippet footer()}
              <Button
                variant="ghost"
                size="sm"
                onclick={() => void copy(item)}
                data-testid="developer-exposed-copy"
              >
                {#snippet icon()}<Copy size={14} aria-hidden="true" />{/snippet}
                {$t("dev.developer.models.copy_id")}
              </Button>
            {/snippet}
          </ServedCard>
        {/each}
      </div>
    {/if}
  </section>
{/if}
