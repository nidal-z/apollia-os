<script lang="ts">
  /**
   * McpOverviewTab - connection status, activity, and the live-test verdict.
   *
   * The state of the connection as the runtime found it, dated; the
   * developer mode adds the technical detail of its reason and of a test.
   */
  import { t } from "svelte-i18n";
  import { Wrench } from "lucide-svelte";
  import { Card } from "$lib/components/ui/card";
  import { developerMode } from "$lib/stores/developerMode";
  import ConnectionStateBadge from "../shared/ConnectionStateBadge.svelte";
  import type { Connection } from "$lib/types";
  import type { TestState } from "../shared/types";

  interface Props {
    /** The connection of the server, as the list reads it; absent before the list is read. */
    connection?: Connection | null;
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
    connection = null,
    toolsCount,
    statusError,
    lastActivityLabel,
    testState,
    testToolCount,
    testDetail,
    testError,
    reconnectError,
  }: Props = $props();

  const isOperator = $derived(!$developerMode);
</script>

<div class="max-w-3xl space-y-4">
  <Card class="px-4 py-3.5">
    <div class="mb-2 text-overline text-muted-foreground/60">
      {$t("connections.overview_connection")}
    </div>
    {#if connection}
      <ConnectionStateBadge
        status={connection.status}
        reason={connection.reason}
        checkedAt={connection.checkedAt}
        inline
        data-testid="mcp-overview-state"
      />
    {/if}
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
