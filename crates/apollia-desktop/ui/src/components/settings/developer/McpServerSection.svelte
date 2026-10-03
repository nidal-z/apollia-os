<script lang="ts">
  /**
   * McpServerSection - Apollia as an MCP server, in Settings > Developer.
   *
   * The switch (`mcp.server_enabled`, off by default), then, switched on, the
   * agents and workflows with an "exposed" checkbox each, and the
   * configuration to paste into an MCP client with its Copy button.
   */
  import { t } from "svelte-i18n";
  import { Check, Copy, Lock, Plug } from "lucide-svelte";
  import { Toggle } from "$lib/components/ui/toggle";
  import { Checkbox } from "$lib/components/ui/checkbox";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { addToast } from "$lib/components/ui/toast";
  import { getResolvedSetting, setSetting } from "$lib/ipc/settings";
  import { listMcpExposable, setMcpExposed, type McpExposableItem } from "$lib/ipc/mcpServer";
  import { cliStatusStore, settingsLoaders } from "$lib/stores/settings";
  import type { CliStatus } from "$lib/types";
  import SettingsSection from "../SettingsSection.svelte";
  import SettingsFieldRow from "../SettingsFieldRow.svelte";
  import {
    exposedTestId,
    mcpClientConfig,
    mcpCommandPath,
    readMcpSwitch,
    setExposure,
    writeMcpSwitch,
    type McpServerIpc,
  } from "./mcpServer";

  const KIND_LABELS = {
    agent: "settings.developer.mcp.kind_agent",
    workflow: "settings.developer.mcp.kind_workflow",
  } as const;

  const ipc: McpServerIpc = {
    readSetting: getResolvedSetting,
    writeSetting: setSetting,
    listItems: listMcpExposable,
    setExposed: setMcpExposed,
  };

  let enabled = $state(false);
  let locked = $state(false);
  let items = $state<McpExposableItem[]>([]);
  let itemsLoaded = $state(false);
  let itemsError = $state<string | null>(null);
  let pending = $state<string | null>(null);
  // Bumped on a refused write, so each checkbox is rebuilt on its stored flag.
  let revision = $state(0);
  let copied = $state(false);

  const commandPath = $derived(mcpCommandPath($cliStatusStore.data as CliStatus | null));
  const config = $derived(commandPath ? mcpClientConfig(commandPath) : null);

  $effect(() => {
    let live = true;
    void readMcpSwitch(ipc).then((read) => {
      if (!live) return;
      enabled = read.enabled;
      locked = read.locked;
    });
    void settingsLoaders.cliStatus();
    return () => {
      live = false;
    };
  });

  $effect(() => {
    if (!enabled) return;
    let live = true;
    itemsError = null;
    ipc
      .listItems()
      .then((listed) => {
        if (!live) return;
        items = listed;
        itemsLoaded = true;
      })
      .catch((err: unknown) => {
        if (!live) return;
        itemsError = err instanceof Error ? err.message : String(err);
        itemsLoaded = true;
      });
    return () => {
      live = false;
    };
  });

  async function toggle(next: boolean) {
    const previous = enabled;
    enabled = next;
    try {
      const written = await writeMcpSwitch(ipc, next);
      enabled = written.enabled;
      locked = written.locked;
    } catch {
      enabled = previous;
      addToast($t("settings.developer.mcp.write_failed"), "error");
    }
  }

  async function expose(item: McpExposableItem, exposed: boolean) {
    const key = exposedTestId(item);
    pending = key;
    try {
      items = await setExposure(ipc, items, item, exposed);
    } catch {
      revision += 1;
      addToast($t("settings.developer.mcp.expose_failed", { values: { name: item.name } }), "error");
    } finally {
      pending = null;
    }
  }

  async function copy() {
    if (!config) return;
    try {
      await navigator.clipboard.writeText(config);
      copied = true;
      setTimeout(() => (copied = false), 2000);
    } catch {
      addToast($t("settings.developer.mcp.copy_failed"), "error");
    }
  }
</script>

<SettingsSection
  title={$t("settings.developer.mcp.title")}
  bodyLayout="flush"
  data-testid="developer-mcp-section"
>
  {#snippet icon()}<Plug size={15} strokeWidth={1.75} />{/snippet}
  <SettingsFieldRow
    label={$t("settings.developer.mcp.switch_label")}
    hint={$t("settings.developer.mcp.switch_hint")}
    for="developer-mcp-switch"
    border={false}
    data-testid="developer-mcp-row"
  >
    {#snippet control()}
      <div class="flex flex-col items-end gap-1">
        <Toggle
          id="developer-mcp-switch"
          checked={enabled}
          disabled={locked}
          onchange={(next) => void toggle(next)}
          aria-label={$t("settings.developer.mcp.switch_label")}
          data-testid="developer-mcp-toggle"
        />
        {#if locked}
          <p
            class="inline-flex items-center gap-1.5 text-xs text-muted-foreground"
            title={$t("settings.policy_locked_hint")}
            data-testid="developer-mcp-policy"
          >
            <Lock class="h-3 w-3" aria-hidden="true" />
            {$t("settings.policy_locked")}
          </p>
        {/if}
      </div>
    {/snippet}
  </SettingsFieldRow>

  {#if enabled}
    <div class="space-y-1 pt-2" data-testid="developer-mcp-items">
      <p class="text-sm font-medium text-foreground">{$t("settings.developer.mcp.items_label")}</p>
      {#if itemsError}
        <p class="text-xs text-destructive" data-testid="developer-mcp-items-error">
          {$t("settings.developer.mcp.items_failed", { values: { error: itemsError } })}
        </p>
      {:else if itemsLoaded && items.length === 0}
        <p class="text-xs text-muted-foreground" data-testid="developer-mcp-items-empty">
          {$t("settings.developer.mcp.items_empty")}
        </p>
      {:else}
        <p class="text-xs text-muted-foreground">{$t("settings.developer.mcp.items_hint")}</p>
        <ul class="divide-y divide-border pt-2">
          {#each items as item (`${item.kind}:${item.item_id}:${revision}`)}
            {@const testid = exposedTestId(item)}
            <li class="flex items-start gap-3 py-3" data-testid={`${testid}-row`}>
              <Checkbox
                checked={item.exposed}
                loading={pending === testid}
                class="mt-0.5"
                onchange={(next) => void expose(item, next)}
                data-testid={testid}
              />
              <div class="min-w-0 flex-1">
                <p class="truncate text-sm text-foreground">{item.name}</p>
                {#if item.description}
                  <p class="line-clamp-2 text-xs text-muted-foreground">{item.description}</p>
                {/if}
              </div>
              <Badge variant="neutral" outline size="sm">
                {$t(KIND_LABELS[item.kind])}
              </Badge>
            </li>
          {/each}
        </ul>
      {/if}
    </div>

    {#if config}
      <div
        class="mt-3 rounded-lg border border-border bg-muted/60 p-3"
        data-testid="developer-mcp-config"
      >
        <div class="flex items-center justify-between gap-3">
          <p class="text-xs text-muted-foreground">{$t("settings.developer.mcp.config_label")}</p>
          <Button variant="outline" size="sm" onclick={() => void copy()} data-testid="developer-mcp-copy">
            {#snippet icon()}
              {#if copied}<Check size={14} />{:else}<Copy size={14} />{/if}
            {/snippet}
            {copied ? $t("settings.developer.mcp.copied") : $t("settings.developer.mcp.copy")}
          </Button>
        </div>
        <pre
          class="mt-2 overflow-x-auto font-mono text-xs text-foreground"
          data-testid="developer-mcp-config-text">{config}</pre>
      </div>
    {/if}
  {/if}
</SettingsSection>
