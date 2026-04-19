<script lang="ts">
  import { t } from "svelte-i18n";
  import { invoke } from "@tauri-apps/api/core";
  import { Spinner } from "$lib/components/ui/progress";
  import { Sheet } from "$lib/components/ui/sheet";
  import { Button } from "$lib/components/ui/button";
  import { TabBar } from "$lib/components/ui/tabs";
  import type { McpServerDetailView } from "$lib/types";
  import ConnectionStatusIndicator from "./ConnectionStatusIndicator.svelte";
  import BuilderToolsList from "./BuilderToolsList.svelte";
  import BuilderServerLogs from "./BuilderServerLogs.svelte";

  type ActiveTab = "tools" | "logs" | "config";

  interface Props {
    serverName: string;
    open: boolean;
    onclose: () => void;
    onRemove: () => void;
  }

  let { serverName, open, onclose, onRemove }: Props = $props();

  let detail = $state<McpServerDetailView | null>(null);
  let loading = $state(false);
  let fetchError = $state<string | null>(null);
  let activeTab = $state<ActiveTab>("tools");

  let restarting = $state(false);
  let restartError = $state<string | null>(null);

  let confirmRemove = $state(false);
  let removing = $state(false);
  let removeError = $state<string | null>(null);

  async function fetchDetail(): Promise<void> {
    loading = true;
    fetchError = null;
    try {
      detail = await invoke<McpServerDetailView>("get_mcp_server_detail", {
        name: serverName,
      });
    } catch (err: unknown) {
      fetchError = err instanceof Error ? err.message : String(err);
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    if (open) {
      detail = null;
      activeTab = "tools";
      restarting = false;
      restartError = null;
      confirmRemove = false;
      removing = false;
      removeError = null;
      fetchDetail();
    }
  });

  async function handleRestart(): Promise<void> {
    restarting = true;
    restartError = null;
    try {
      const updated = await invoke("restart_mcp_server", { name: serverName });
      if (detail) {
        detail = { ...detail, status: updated as McpServerDetailView["status"] };
      }
    } catch (err: unknown) {
      restartError = err instanceof Error ? err.message : String(err);
    } finally {
      restarting = false;
    }
  }

  async function handleRemove(): Promise<void> {
    removing = true;
    removeError = null;
    try {
      await invoke("remove_mcp_server", { name: serverName });
      onRemove();
    } catch (err: unknown) {
      removeError = err instanceof Error ? err.message : String(err);
      removing = false;
    }
  }

  const tabs = $derived([
    {
      key: "tools",
      label: $t("integrations.builder.detail.tab_tools"),
      count: detail?.tools.length,
    },
    { key: "logs", label: $t("integrations.builder.detail.tab_logs") },
    { key: "config", label: $t("integrations.builder.detail.tab_config") },
  ]);
</script>

<Sheet {open} {onclose}>
  <div class="flex h-full flex-col overflow-hidden">
    <!-- Header -->
    <div class="shrink-0 px-6 pt-6 pb-4 pr-12">
      <h2 class="text-base font-semibold text-foreground" data-testid="builder-detail-title">
        {serverName}
      </h2>
      {#if detail}
        <div class="mt-1">
          <ConnectionStatusIndicator
            connected={detail.status.connected}
            error={detail.status.error}
          />
        </div>
      {/if}
    </div>

    {#if loading}
      <div
        class="flex flex-1 items-center gap-2 px-6 text-sm text-muted-foreground"
        data-testid="builder-detail-loading"
      >
        <Spinner size={14} />
        <span>{$t("common.loading")}</span>
      </div>
    {:else if fetchError}
      <p class="px-6 text-sm text-destructive" data-testid="builder-detail-error">
        {fetchError}
      </p>
    {:else if detail}
      <!-- Tab bar -->
      <div class="shrink-0 px-6 pb-3">
        <TabBar
          items={tabs}
          activeTab={activeTab}
          ontabchange={(key) => { activeTab = key as ActiveTab; }}
          testidPrefix="builder-detail"
        />
      </div>

      <!-- Tab content -->
      <div class="min-h-0 flex-1 overflow-y-auto px-6 pb-4">
        {#if activeTab === "tools"}
          <BuilderToolsList tools={detail.tools} />

        {:else if activeTab === "logs"}
          <BuilderServerLogs serverName={serverName} />

        {:else if activeTab === "config"}
          <dl class="flex flex-col gap-3 text-sm">
            <!-- Command -->
            <div>
              <dt class="text-xs font-medium uppercase tracking-wide text-muted-foreground mb-1">
                {$t("integrations.builder.detail.command")}
              </dt>
              <dd>
                <code
                  class="block rounded bg-muted px-3 py-2 font-mono text-xs text-foreground break-all"
                  data-testid="config-command"
                >
                  {detail.config.command}
                  {#if detail.config.args.length > 0}
                    {" "}{detail.config.args.join(" ")}
                  {/if}
                </code>
              </dd>
            </div>

            <!-- Transport -->
            <div>
              <dt class="text-xs font-medium uppercase tracking-wide text-muted-foreground mb-1">
                {$t("integrations.builder.detail.transport")}
              </dt>
              <dd>
                <span
                  class="rounded bg-muted px-1.5 py-0.5 font-mono text-xs text-muted-foreground"
                  data-testid="config-transport"
                >
                  {detail.config.transport}
                </span>
              </dd>
            </div>

            <!-- Environment variables -->
            <div>
              <dt class="text-xs font-medium uppercase tracking-wide text-muted-foreground mb-1">
                {$t("integrations.builder.detail.env_keys")}
              </dt>
              <dd>
                {#if detail.config.env_keys.length === 0}
                  <p class="text-xs text-muted-foreground" data-testid="config-no-env">
                    {$t("integrations.builder.detail.no_env")}
                  </p>
                {:else}
                  <ul class="flex flex-col gap-1" data-testid="config-env-list">
                    {#each detail.config.env_keys as key (key)}
                      <li class="flex items-center justify-between rounded bg-muted px-3 py-1.5">
                        <span class="font-mono text-xs text-foreground">{key}</span>
                        <span
                          class="font-mono text-xs text-muted-foreground"
                          aria-label={$t("a11y.value_redacted")}
                        >
                          ***
                        </span>
                      </li>
                    {/each}
                  </ul>
                {/if}
              </dd>
            </div>

            <!-- Tags -->
            {#if detail.config.tags.length > 0}
              <div>
                <dt class="text-xs font-medium uppercase tracking-wide text-muted-foreground mb-1">
                  {$t("integrations.builder.detail.tags")}
                </dt>
                <dd class="flex flex-wrap gap-1" data-testid="config-tags">
                  {#each detail.config.tags as tag (tag)}
                    <span class="rounded bg-muted px-1.5 py-0.5 text-xs text-muted-foreground">
                      {tag}
                    </span>
                  {/each}
                </dd>
              </div>
            {/if}
          </dl>
        {/if}
      </div>

      <!-- Actions footer -->
      <div class="shrink-0 border-t border-border px-6 py-4 flex flex-col gap-2">
        {#if restartError}
          <p class="text-xs text-destructive" data-testid="builder-restart-error">
            {restartError}
          </p>
        {/if}

        {#if !confirmRemove}
          <div class="flex gap-2">
            <Button
              size="sm"
              variant="outline"
              class="flex-1"
              onclick={handleRestart}
              disabled={restarting}
              data-testid="builder-restart-btn"
            >
              {#if restarting}
                <Spinner size={14} class="mr-1.5" />
                {$t("integrations.builder.detail.restarting")}
              {:else}
                {$t("integrations.builder.detail.restart_button")}
              {/if}
            </Button>
            <Button
              size="sm"
              variant="destructive"
              class="flex-1"
              onclick={() => { confirmRemove = true; }}
              data-testid="builder-remove-btn"
            >
              {$t("integrations.builder.detail.remove_button")}
            </Button>
          </div>
        {:else}
          <div
            class="rounded-lg border border-destructive/30 bg-destructive/5 px-4 py-3 flex flex-col gap-3"
            data-testid="builder-remove-confirm"
          >
            <p class="text-sm text-foreground">
              {$t("integrations.builder.detail.remove_warning", {
                values: { name: serverName },
              })}
            </p>
            {#if removeError}
              <p class="text-xs text-destructive">{removeError}</p>
            {/if}
            <div class="flex gap-2">
              <Button
                size="sm"
                variant="outline"
                class="flex-1"
                onclick={() => { confirmRemove = false; removeError = null; }}
                disabled={removing}
                data-testid="builder-remove-cancel"
              >
                {$t("integrations.builder.detail.remove_cancel")}
              </Button>
              <Button
                size="sm"
                variant="destructive"
                class="flex-1"
                onclick={handleRemove}
                disabled={removing}
                data-testid="builder-remove-confirm-btn"
              >
                {#if removing}
                  <Spinner size={14} class="mr-1.5" />
                  {$t("integrations.builder.detail.removing")}
                {:else}
                  {$t("integrations.builder.detail.remove_confirm")}
                {/if}
              </Button>
            </div>
          </div>
        {/if}
      </div>
    {/if}
  </div>
</Sheet>
