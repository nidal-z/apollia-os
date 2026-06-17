<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import type { LlmBackendConfig, LlmPingResult } from "$lib/types";
  import { uiMode } from "$lib/stores/mode";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { EntityCard } from "$lib/components/operator";

  interface Props {
    backend: LlmBackendConfig;
  }

  let { backend }: Props = $props();

  let pinging = $state(false);
  let pingResult = $state<number | null>(null);
  let pingError = $state<string | null>(null);

  type StatusKey = "ready" | "error";
  type TypeKey = "embedded" | "api";

  const STATUS_BADGE: Record<
    StatusKey,
    { labelKey: string; variant: "success" | "destructive" }
  > = {
    ready: { labelKey: "common.status.ready", variant: "success" },
    error: { labelKey: "common.status.error", variant: "destructive" },
  };

  const STATUS_ACCENT: Record<StatusKey, "primary" | "destructive"> = {
    ready: "primary",
    error: "destructive",
  };

  const TYPE_BADGE_VARIANT: Record<TypeKey, "info" | "outline"> = {
    embedded: "info",
    api: "outline",
  };

  const TYPE_LABEL: Record<TypeKey, string> = {
    embedded: "EMBEDDED",
    api: "API",
  };

  /** `"llama-cpp"` runs locally; all other providers are remote API calls. */
  const backendType = $derived<TypeKey>(
    backend.provider === "llama-cpp" ? "embedded" : "api",
  );

  /** Derive a display status from the `enabled` flag. */
  const statusKey = $derived<StatusKey>(backend.enabled ? "ready" : "error");

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
  const statusBadge = $derived(STATUS_BADGE[statusKey]);
  const statusAccent = $derived(STATUS_ACCENT[statusKey]);

  function prettifyModelName(model: string): string {
    return model
      .replace(/[-_]/g, " ")
      .replace(/\b\w/g, (c) => c.toUpperCase());
  }

  const humanizedTitle = $derived.by(() => {
    const isLocal = backendType === "embedded";
    const modelName = prettifyModelName(backend.model || backend.name);
    const status = backend.enabled ? $t("llm.running") : $t("llm.status_error");
    return isLocal
      ? `${$t("llm.local_ai")} (${modelName}) - ${status}`
      : `${modelName} - ${status}`;
  });

  const humanizedCost = $derived.by(() => {
    return backendType === "embedded" ? $t("llm.free_local") : $t("llm.pay_per_use");
  });
</script>

{#snippet builderBadges()}
  {#if backend.is_default}
    <Badge variant="outline" class="border-primary text-primary text-[10px]">
      {$t("common.default_badge")}
    </Badge>
  {/if}
  <Badge variant={TYPE_BADGE_VARIANT[backendType]} data-testid="llm-backend-type-badge">
    {TYPE_LABEL[backendType]}
  </Badge>
  <Badge variant={statusBadge.variant} data-testid="llm-backend-badge">
    {$t(statusBadge.labelKey)}
  </Badge>
{/snippet}

{#snippet operatorBadges()}
  <Badge
    variant="outline"
    class={backendType === 'embedded' ? 'border-success text-success' : ''}
    data-testid="llm-backend-badge"
  >
    {humanizedCost}
  </Badge>
{/snippet}

{#snippet builderBody()}
  <p class="text-[11px] text-muted-foreground">
    {$t('llm.model')}: {backend.model}
  </p>

  <!-- Ping feedback -->
  {#if pingResult !== null}
    <p class="text-[11px] text-success" data-testid="llm-ping-result">
      {$t('llm.ping_ok', { values: { latency: pingResult } })}
    </p>
  {/if}
  {#if pingError}
    <p class="text-[11px] text-destructive" data-testid="llm-ping-result">
      {pingError}
    </p>
  {/if}
{/snippet}

{#snippet builderActions()}
  <Button
    size="sm"
    variant="outline"
    onclick={handlePing}
    disabled={pinging}
    data-testid="llm-backend-ping-btn"
  >
    {pinging ? $t('llm.pinging') : $t('llm.ping')}
  </Button>
{/snippet}

<EntityCard
  accent={statusAccent}
  title={isBuilder ? backend.name : humanizedTitle}
  badges={isBuilder ? builderBadges : operatorBadges}
  body={isBuilder ? builderBody : undefined}
  actions={isBuilder ? builderActions : undefined}
  data-testid="llm-backend-card"
/>
