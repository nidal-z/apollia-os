<script lang="ts">
  /**
   * ServerPanel - one saved server of the AI screen: its head (status, the
   * switch that turns it on or off, test, change, and the menu that copies its
   * address or removes it), then its models, each with the window the server
   * serves it with and the menu that makes it the default model.
   *
   * A server that stops answering keeps the last list it gave, greyed, under
   * a banner that offers to try again. The loading settings of a model stay
   * with whoever runs the server: nothing here changes them.
   */
  import { locale, t } from "svelte-i18n";
  import { Copy, Ellipsis, Pencil, PlugZap, Star, Trash2 } from "lucide-svelte";
  import type { DeclaredServer, DefaultModel, ServerModel } from "$lib/ipc/llmServers";
  import { ActionMenu } from "$lib/components/ui/action-menu";
  import { Banner } from "$lib/components/ui/banner";
  import { Button } from "$lib/components/ui/button";
  import { Spinner } from "$lib/components/ui/progress";
  import { Toggle } from "$lib/components/ui/toggle";
  import { ErrorBanner } from "$lib/components/operator";
  import { cn } from "$lib/utils";
  import { SERVER_FAILURE_TEXT } from "../onboarding/serverSetup";
  import ExplainedBadge from "./ExplainedBadge.svelte";
  import {
    SERVER_STATUS_LABEL,
    isDefaultModel,
    serverStatus,
    tokenCount,
    type ServerModelsState,
    type ServerStatus,
  } from "./servers";

  interface Props {
    server: DeclaredServer;
    models: ServerModelsState | undefined;
    defaultModel: DefaultModel | null;
    /** Whether a gesture is in flight on this server. */
    busy: boolean;
    ontest: () => void;
    onedit: () => void;
    ontoggle: (enabled: boolean) => void;
    oncopyaddress: () => void;
    ondelete: () => void;
    onusedefault: (model: ServerModel) => void;
    oncopymodel: (model: ServerModel) => void;
  }

  let {
    server,
    models,
    defaultModel,
    busy,
    ontest,
    onedit,
    ontoggle,
    oncopyaddress,
    ondelete,
    onusedefault,
    oncopymodel,
  }: Props = $props();

  const STATUS_VARIANT: Record<ServerStatus, "neutral" | "success" | "warning"> = {
    active: "success",
    disabled: "neutral",
    checking: "neutral",
    unreachable: "warning",
    failing: "warning",
  };
  const STATUS_EXPLANATION: Record<ServerStatus, string> = {
    active: "ai.servers.status.active_explanation",
    disabled: "ai.servers.status.disabled_explanation",
    checking: "ai.servers.status.checking_explanation",
    unreachable: "ai.servers.status.unreachable_explanation",
    failing: "ai.servers.status.failing_explanation",
  };

  const lang = $derived($locale ?? "fr");
  const status = $derived(serverStatus(server, models));
  const listed = $derived(models?.models ?? []);
  const failed = $derived(models?.phase === "failed" ? models.failure : null);
  const checking = $derived(models?.phase === "loading");
  const stale = $derived(failed !== null);
</script>

<section class="flex min-h-0 flex-col gap-4" data-testid="ai-server-panel" data-server={server.name}>
  <header class="flex flex-wrap items-center gap-2">
    <h2 class="text-sm font-semibold text-foreground" data-testid="ai-server-name-title">{server.name}</h2>
    <ExplainedBadge
      label={$t(SERVER_STATUS_LABEL[status])}
      explanation={$t(STATUS_EXPLANATION[status])}
      variant={STATUS_VARIANT[status]}
      testid="ai-server-status"
    />
    <div class="ml-auto flex items-center gap-2">
      <label class="flex items-center gap-2 text-sm text-foreground" for="ai-server-enabled">
        {$t("ai.servers.enabled_label")}
        <!-- Rebuilt after each gesture: a refused change leaves the switch where the server is. -->
        {#key `${server.enabled}:${busy}`}
          <Toggle
            id="ai-server-enabled"
            size="sm"
            checked={server.enabled}
            loading={busy}
            onchange={ontoggle}
            aria-label={$t("ai.servers.enabled_aria", { values: { name: server.name } })}
            data-testid="ai-server-enabled"
          />
        {/key}
      </label>
      <Button
        variant="outline"
        size="sm"
        onclick={ontest}
        disabled={!server.enabled || checking}
        data-testid="ai-server-retest"
      >
        {#if checking}<Spinner size={12} />{:else}<PlugZap size={14} aria-hidden="true" />{/if}
        {$t("ai.servers.test")}
      </Button>
      <Button variant="outline" size="sm" onclick={onedit} data-testid="ai-server-edit">
        <Pencil size={14} aria-hidden="true" />
        {$t("ai.servers.edit")}
      </Button>
      <ActionMenu
        align="end"
        triggerLabel={$t("ai.servers.menu_more", { values: { name: server.name } })}
        data-testid="ai-server-menu-btn"
        items={[
          {
            id: "copy_address",
            label: $t("ai.servers.menu.copy_address"),
            icon: Copy,
            onclick: oncopyaddress,
            testid: "ai-server-menu-copy-address",
          },
          {
            id: "delete",
            label: $t("ai.servers.menu.delete"),
            icon: Trash2,
            variant: "destructive",
            onclick: ondelete,
            testid: "ai-server-menu-delete",
          },
        ]}
      />
    </div>
  </header>

  <dl class="flex flex-wrap items-center gap-x-6 gap-y-1 border-b border-border pb-4 text-sm">
    <div class="flex items-center gap-2">
      <dt class="text-muted-foreground">{$t("ai.servers.address")}</dt>
      <dd>
        <code class="rounded border border-border px-2 py-0.5 text-xs text-foreground" data-testid="ai-server-address-value">
          {server.base_url}
        </code>
      </dd>
    </div>
    <div class="flex items-center gap-2">
      <dt class="text-muted-foreground">{$t("ai.servers.key")}</dt>
      <dd class="text-foreground" data-testid="ai-server-key-state">
        {server.has_key || server.api_key_env ? $t("ai.servers.key_saved") : $t("ai.servers.key_none")}
      </dd>
    </div>
    {#if server.enabled && models?.phase === "listed"}
      <div class="text-foreground" data-testid="ai-server-model-count">
        {$t("ai.servers.model_count", { values: { count: listed.length } })}
      </div>
    {/if}
  </dl>

  {#if !server.enabled}
    <p class="text-sm text-muted-foreground" data-testid="ai-server-disabled">
      {$t("ai.servers.disabled_hint")}
    </p>
  {:else}
    {#if failed}
      {@const text = SERVER_FAILURE_TEXT[failed.kind]}
      <ErrorBanner onretry={ontest} loading={checking} retryLabel={$t("common.retry")} data-testid="ai-server-error">
        <div class="flex flex-col gap-1">
          <span class="font-semibold text-foreground">{$t(text.title)}</span>
          <span class="text-muted-foreground">{$t(text.body)}</span>
          {#if failed.detail}
            <span class="break-words text-xs text-muted-foreground" data-testid="ai-server-error-detail">
              {failed.detail}
            </span>
          {/if}
        </div>
      </ErrorBanner>
    {/if}

    {#if models === undefined || (checking && listed.length === 0)}
      <p class="flex items-center gap-2 text-sm text-muted-foreground" role="status" data-testid="ai-server-loading">
        <Spinner size={12} />
        {$t("ai.servers.loading")}
      </p>
    {:else if listed.length === 0 && !failed}
      <p class="text-sm text-muted-foreground" data-testid="ai-server-no-models">
        {$t("ai.servers.no_models")}
      </p>
    {:else if listed.length > 0}
      {#if stale}
        <p class="text-sm text-muted-foreground" data-testid="ai-server-stale">
          {$t("ai.servers.stale_hint")}
        </p>
      {/if}
      <div class="min-h-0 overflow-auto rounded-lg border border-border" data-testid="ai-server-models">
        <table class={cn("w-full text-sm", stale && "text-muted-foreground")}>
          <thead class="bg-muted text-xs text-muted-foreground">
            <tr>
              <th class="px-3 py-2 text-left font-medium">{$t("ai.servers.col_model")}</th>
              <th class="px-3 py-2 text-left font-medium">{$t("ai.servers.col_window")}</th>
              <th class="px-3 py-2 text-right font-medium">{$t("ai.installed.col_actions")}</th>
            </tr>
          </thead>
          <tbody>
            {#each listed as model (model.id)}
              {@const isDefault = isDefaultModel(defaultModel, server.name, model.id)}
              <tr class="border-t border-border" data-testid="ai-server-model-row" data-model-id={model.id}>
                <td class="px-3 py-2">
                  <div class="flex flex-wrap items-center gap-1.5">
                    <span class={cn("font-medium", !stale && "text-foreground")} data-testid="ai-server-model-name">
                      {model.id}
                    </span>
                    {#if isDefault}
                      <ExplainedBadge
                        label={$t("ai.badge.default")}
                        explanation={$t("ai.badge.default_explanation")}
                        variant="primary"
                        testid="ai-server-model-default"
                      />
                    {/if}
                    {#if model.context_short}
                      <ExplainedBadge
                        label={$t("ai.servers.short_window")}
                        explanation={$t("ai.servers.short_window_explanation")}
                        variant="warning"
                        testid="ai-server-model-short"
                      />
                    {/if}
                  </div>
                </td>
                <td class="whitespace-nowrap px-3 py-2" data-testid="ai-server-model-window">
                  {model.context_window === null
                    ? $t("ai.servers.window_unknown")
                    : $t("ai.servers.window", {
                        values: { tokens: tokenCount(model.context_window, lang) },
                      })}
                </td>
                <td class="px-3 py-2 text-right">
                  {#if !stale}
                    <ActionMenu
                      align="end"
                      items={[
                        {
                          id: "use_default",
                          label: $t("ai.menu.use_default"),
                          icon: Star,
                          disabled: isDefault || busy,
                          onclick: () => onusedefault(model),
                          testid: "ai-server-model-menu-use-default",
                        },
                        {
                          id: "copy_id",
                          label: $t("ai.menu.copy_id"),
                          icon: Copy,
                          onclick: () => oncopymodel(model),
                          testid: "ai-server-model-menu-copy-id",
                        },
                      ]}
                    >
                      {#snippet triggerSlot(props)}
                        <button
                          type="button"
                          {...props}
                          class="inline-flex h-7 w-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-muted hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
                          aria-label={$t("ai.menu.more", { values: { name: model.id } })}
                          data-testid="ai-server-model-menu-btn"
                        >
                          <Ellipsis size={15} strokeWidth={1.75} aria-hidden="true" />
                        </button>
                      {/snippet}
                    </ActionMenu>
                  {/if}
                </td>
              </tr>
              {#if model.context_short && model.context_window !== null && !stale}
                <tr data-testid="ai-server-model-short-warning">
                  <td colspan="3" class="px-3 pb-3">
                    <Banner variant="warning" surface="card">
                      {$t("ai.servers.short_window_warning", {
                        values: {
                          model: model.id,
                          window: tokenCount(model.context_window, lang),
                          expected: tokenCount(models.expectedWindow ?? model.context_window, lang),
                        },
                      })}
                    </Banner>
                  </td>
                </tr>
              {/if}
            {/each}
          </tbody>
        </table>
      </div>
      <p class="text-xs text-muted-foreground" data-testid="ai-server-admin-note">
        {$t("ai.servers.admin_note")}
      </p>
    {/if}
  {/if}
</section>
