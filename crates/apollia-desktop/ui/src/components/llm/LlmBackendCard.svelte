<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import type { LlmBackendStatus, LlmPingResult } from "$lib/types";
  import { uiMode } from "$lib/stores/mode";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";

  interface Props {
    backend: LlmBackendStatus;
  }

  let { backend }: Props = $props();

  let pinging = $state(false);
  let pingResult = $state<number | null>(null);
  let pingError = $state<string | null>(null);

  const STATUS_BADGE: Record<
    LlmBackendStatus["status"],
    { labelKey: string; variant: "success" | "warning" | "destructive" }
  > = {
    ready: { labelKey: "common.status.ready", variant: "success" },
    loading: { labelKey: "common.status.loading", variant: "warning" },
    error: { labelKey: "common.status.error", variant: "destructive" },
  };

  const STATUS_BAR_COLOR: Record<LlmBackendStatus["status"], string> = {
    ready: "bg-primary",
    loading: "bg-warning",
    error: "bg-destructive",
  };

  const TYPE_BADGE_VARIANT: Record<LlmBackendStatus["backend_type"], "info" | "outline"> = {
    embedded: "info",
    api: "outline",
  };

  const TYPE_LABEL: Record<LlmBackendStatus["backend_type"], string> = {
    embedded: "EMBEDDED",
    api: "API",
  };

  async function handlePing() {
    pinging = true;
    pingResult = null;
    pingError = null;
    try {
      const result: LlmPingResult = await invoke("ping_llm_backend", { name: backend.name });
      if (result.available && result.latency_ms !== null) {
        pingResult = result.latency_ms;
      } else {
        pingError = result.error ?? $t('llm.ping_failed');
      }
    } catch (err: unknown) {
      pingError = err instanceof Error ? err.message : String(err);
    } finally {
      pinging = false;
    }
  }

  const isBuilder = $derived($uiMode === "builder");
  const statusBadge = $derived(STATUS_BADGE[backend.status]);
  const statusBarColor = $derived(STATUS_BAR_COLOR[backend.status]);

  function prettifyModelName(model: string): string {
    return model
      .replace(/[-_]/g, " ")
      .replace(/\b\w/g, (c) => c.toUpperCase());
  }

  const humanizedTitle = $derived.by(() => {
    const isLocal = backend.backend_type === "embedded";
    const modelName = prettifyModelName(backend.model || backend.name);
    const status = backend.status === "ready"
      ? $t("llm.running")
      : $t("llm.status_error");
    return isLocal
      ? `${$t("llm.local_ai")} (${modelName}) — ${status}`
      : `${modelName} — ${status}`;
  });

  const humanizedCost = $derived.by(() => {
    return backend.backend_type === "embedded"
      ? $t("llm.free_local")
      : $t("llm.pay_per_use");
  });
</script>

<div class="glass-card-hover relative overflow-hidden" data-testid="llm-backend-card">
  <!-- Status accent bar -->
  <div class="h-0.5 w-full {statusBarColor}" data-testid="llm-backend-status-bar" />

  <div class="px-3.5 pt-3 pb-2.5">
    {#if isBuilder}
      <!-- Builder mode: full technical card -->
      <div class="flex items-center justify-between">
        <h3 class="text-[13px] font-medium">{backend.name}</h3>
        <div class="flex items-center gap-1.5">
          <Badge variant={TYPE_BADGE_VARIANT[backend.backend_type]} data-testid="llm-backend-type-badge">
            {TYPE_LABEL[backend.backend_type]}
          </Badge>
          <Badge variant={statusBadge.variant} data-testid="llm-backend-badge">
            {$t(statusBadge.labelKey)}
          </Badge>
        </div>
      </div>

      <p class="mt-1 text-[11px] text-muted-foreground">
        {$t('llm.model')}: {backend.model}
      </p>

      <!-- Ping feedback -->
      {#if pingResult !== null}
        <p class="mt-1 text-[11px] text-success" data-testid="llm-ping-result">
          {$t('llm.ping_ok', { values: { latency: pingResult } })}
        </p>
      {/if}
      {#if pingError}
        <p class="mt-1 text-[11px] text-destructive" data-testid="llm-ping-result">
          {pingError}
        </p>
      {/if}

      <!-- Actions -->
      <div class="mt-3 flex gap-2">
        <Button
          size="sm"
          variant="outline"
          onclick={handlePing}
          disabled={pinging}
          data-testid="llm-backend-ping-btn"
        >
          {pinging ? $t('llm.pinging') : $t('llm.ping')}
        </Button>
      </div>
    {:else}
      <!-- Operator mode: humanized card -->
      <h3 class="text-[13px] font-medium">{humanizedTitle}</h3>
      <div class="mt-2">
        <Badge
          variant="outline"
          class={backend.backend_type === 'embedded' ? 'border-success text-success' : ''}
          data-testid="llm-backend-badge"
        >
          {humanizedCost}
        </Badge>
      </div>
    {/if}
  </div>
</div>
