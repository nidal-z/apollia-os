<script lang="ts">
  /**
   * McpOverviewTab - connection status, activity, and the live-test verdict.
   *
   * Operator sees plain-language phrasing; Builder sees the raw health verdict
   * (via ConnectionStatusIndicator) plus technical test detail.
   */
  import { t } from "svelte-i18n";
  import { Wrench } from "lucide-svelte";
  import { Card } from "$lib/components/ui/card";
  import { uiMode } from "$lib/stores/mode";
  import ConnectionStatusIndicator from "../../integrations/ConnectionStatusIndicator.svelte";
  import type { McpHealth } from "$lib/types";
  import type { TestState } from "../shared/types";

  interface Props {
    health: McpHealth;
    toolsCount: number;
    statusError: string | null;
    lastActivityLabel: string | null;
    testState: TestState;
    testToolCount: number;
    testDetail: string | null;
    testError: string | null;
    reconnectError: string | null;
  }

  let {
    health,
    toolsCount,
    statusError,
    lastActivityLabel,
    testState,
    testToolCount,
    testDetail,
    testError,
    reconnectError,
  }: Props = $props();

  const isOperator = $derived($uiMode === "operator");
</script>

<div class="max-w-3xl space-y-4">
  <Card class="px-4 py-3.5">
    <div class="mb-2 text-overline text-muted-foreground/60">
      {$t("connections.overview_connection")}
    </div>
    <ConnectionStatusIndicator {health} />
    <div class="mt-2 flex items-center gap-1.5 text-caption text-muted-foreground">
      <Wrench size={11} />
      <span data-testid="mcp-tools-count">
        {$t("integrations.manage.tools_count", { values: { count: toolsCount } })}
      </span>
    </div>
    {#if statusError}
      <p class="mt-2 text-caption text-destructive" data-testid="mcp-error">{statusError}</p>
    {/if}
  </Card>

  <Card class="px-4 py-3.5">
    <div class="mb-2 text-overline text-muted-foreground/60">
      {$t("connections.overview_activity")}
    </div>
    {#if lastActivityLabel !== null}
      <p class="text-body-sm text-foreground" data-testid="mcp-last-activity">
        {$t("integrations.manage.last_call", { values: { time: lastActivityLabel } })}
      </p>
    {:else}
      <p class="text-body-sm italic text-muted-foreground" data-testid="mcp-no-activity">
        {$t("integrations.manage.no_activity")}
      </p>
    {/if}
  </Card>

  {#if testState === "ok"}
    <p class="text-body-sm text-success" data-testid="mcp-test-ok">
      {$t("integrations.manage.test_working", { values: { count: testToolCount } })}
    </p>
  {:else if testState === "unverified"}
    <p class="text-body-sm text-muted-foreground" data-testid="mcp-test-unverified">
      {$t("integrations.manage.test_reachable", { values: { count: testToolCount } })}
    </p>
  {:else if testState === "degraded" || testState === "needs_reauth"}
    <p class="text-body-sm text-warning-a11y" data-testid="mcp-test-attention">
      {#if isOperator}
        {$t("integrations.manage.test_operator_attention")}
      {:else}
        {$t(
          testState === "needs_reauth"
            ? "integrations.manage.test_needs_reauth"
            : "integrations.manage.test_degraded",
        )}{#if testDetail}
          {" "}- {testDetail}{/if}
      {/if}
    </p>
  {:else if testState === "error"}
    <p class="text-body-sm text-destructive" data-testid="mcp-test-error">
      {testError ?? $t("integrations.manage.test_failed")}
    </p>
  {/if}
  {#if reconnectError}
    <p class="text-body-sm text-destructive" data-testid="mcp-reconnect-error">{reconnectError}</p>
  {/if}
</div>
