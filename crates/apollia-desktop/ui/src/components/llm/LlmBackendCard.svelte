<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import type { LlmBackendStatus, LlmPingResult } from "$lib/types";
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
    { label: string; variant: "default" | "secondary" | "destructive" | "outline"; extraClass: string }
  > = {
    ready: { label: "READY", variant: "default", extraClass: "bg-[var(--apollia-success)] text-white" },
    loading: { label: "LOADING", variant: "outline", extraClass: "animate-pulse border-blue-500 text-blue-500" },
    error: { label: "ERROR", variant: "destructive", extraClass: "" },
  };

  const TYPE_CONFIG: Record<
    LlmBackendStatus["backend_type"],
    { label: string; extraClass: string }
  > = {
    embedded: { label: "EMBEDDED", extraClass: "border-blue-500 text-blue-500" },
    api: { label: "API", extraClass: "border-purple-500 text-purple-500" },
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

  const statusConfig = $derived(STATUS_CONFIG[backend.status]);
  const typeConfig = $derived(TYPE_CONFIG[backend.backend_type]);
</script>

<Card class="relative overflow-hidden">
  <CardHeader class="pb-2">
    <div class="flex items-center justify-between">
      <CardTitle class="text-base font-semibold">{backend.name}</CardTitle>
      <div class="flex items-center gap-2">
        <Badge variant="outline" class={typeConfig.extraClass}>
          {typeConfig.label}
        </Badge>
        <Badge variant={statusConfig.variant} class={statusConfig.extraClass}>
          {statusConfig.label}
        </Badge>
      </div>
    </div>
  </CardHeader>

  <CardContent>
    <div class="space-y-3">
      <div class="flex items-center gap-4 text-xs text-muted-foreground">
        <span>Model: {backend.model}</span>
      </div>

      <div class="flex items-center gap-2">
        <Button size="sm" variant="outline" onclick={handlePing} disabled={pinging}>
          {pinging ? "Pinging..." : "Ping"}
        </Button>
        {#if pingResult !== null}
          <span class="text-xs font-medium text-[var(--apollia-success)]">{pingResult}ms</span>
        {/if}
        {#if pingError}
          <span class="text-xs text-[hsl(var(--destructive))]">{pingError}</span>
        {/if}
      </div>
    </div>
  </CardContent>
</Card>
