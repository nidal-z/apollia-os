<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import type { LlmBackendStatus, LlmPingResult } from "$lib/types";
  import { uiMode } from "$lib/stores/mode";
  import { Card, CardContent, CardHeader, CardTitle } from "$lib/components/ui/card";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";

  interface Props {
    backend: LlmBackendStatus;
  }

  let { backend }: Props = $props();

  let pinging = $state(false);
  let pingResult = $state<number | null>(null);
  let pingError = $state<string | null>(null);

  const STATUS_CONFIG: Record<
    LlmBackendStatus["status"],
    { labelKey: string; variant: "default" | "secondary" | "destructive" | "outline"; extraClass: string }
  > = {
    ready: { labelKey: "common.status.ready", variant: "default", extraClass: "bg-[var(--apollia-success)] text-white" },
    loading: { labelKey: "common.status.loading", variant: "outline", extraClass: "animate-pulse border-info text-info" },
    error: { labelKey: "common.status.error", variant: "destructive", extraClass: "" },
  };

  const TYPE_CONFIG: Record<
    LlmBackendStatus["backend_type"],
    { label: string; extraClass: string }
  > = {
    embedded: { label: "EMBEDDED", extraClass: "border-info text-info" },
    api: { label: "API", extraClass: "border-accent text-accent-foreground" },
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
        pingError = result.error ?? "ping failed";
      }
    } catch (err: unknown) {
      pingError = err instanceof Error ? err.message : String(err);
    } finally {
      pinging = false;
    }
  }

  const isOperator = $derived($uiMode === "operator");
  const statusConfig = $derived(STATUS_CONFIG[backend.status]);
  const typeConfig = $derived(TYPE_CONFIG[backend.backend_type]);

  function prettifyModelName(model: string): string {
    return model
      .replace(/[-_]/g, " ")
      .replace(/\b\w/g, (c) => c.toUpperCase());
  }

  const humanizedTitle = $derived(() => {
    const isLocal = backend.backend_type === "embedded";
    const modelName = prettifyModelName(backend.model || backend.name);
    const status = backend.status === "ready"
      ? $t("llm.running")
      : $t("llm.status_error");
    return isLocal
      ? `${$t("llm.local_ai")} (${modelName}) — ${status}`
      : `${modelName} — ${status}`;
  });

  const humanizedCost = $derived(() => {
    return backend.backend_type === "embedded"
      ? $t("llm.free_local")
      : $t("llm.pay_per_use");
  });
</script>

{#if isOperator}
  <!-- Operator mode: humanized card -->
  <Card class="relative overflow-hidden">
    <CardHeader class="pb-2">
      <CardTitle class="text-base font-medium">{humanizedTitle()}</CardTitle>
    </CardHeader>

    <CardContent>
      <Badge variant="outline" class="text-xs {backend.backend_type === 'embedded' ? 'border-success text-success' : 'border-accent text-accent-foreground'}">
        {humanizedCost()}
      </Badge>
    </CardContent>
  </Card>
{:else}
  <!-- Builder mode: full technical card -->
  <Card class="relative overflow-hidden">
    <CardHeader class="pb-2">
      <div class="flex items-center justify-between">
        <CardTitle class="text-base font-medium">{backend.name}</CardTitle>
        <div class="flex items-center gap-2">
          <Badge variant="outline" class={typeConfig.extraClass}>
            {typeConfig.label}
          </Badge>
          <Badge variant={statusConfig.variant} class={statusConfig.extraClass}>
            {$t(statusConfig.labelKey)}
          </Badge>
        </div>
      </div>
    </CardHeader>

    <CardContent>
      <div class="space-y-3">
        <div class="flex items-center gap-4 text-xs text-muted-foreground">
          <span>{$t('llm.model')}: {backend.model}</span>
        </div>

        <div class="flex items-center gap-2">
          <Button size="sm" variant="outline" onclick={handlePing} disabled={pinging}>
            {pinging ? $t('llm.pinging') : $t('llm.ping')}
          </Button>
          {#if pingResult !== null}
            <span class="text-xs font-medium text-[var(--apollia-success)]">{pingResult}ms</span>
          {/if}
          {#if pingError}
            <span class="text-xs text-destructive">{pingError}</span>
          {/if}
        </div>
      </div>
    </CardContent>
  </Card>
{/if}
