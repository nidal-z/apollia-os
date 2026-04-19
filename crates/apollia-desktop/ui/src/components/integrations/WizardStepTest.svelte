<script lang="ts">
  import { t } from "svelte-i18n";
  import { CheckCircle2, XCircle } from "lucide-svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { Button } from "$lib/components/ui/button";
  import { Spinner } from "$lib/components/ui/progress";
  import type { McpConnectionTestResultView, McpServerConfigInput } from "$lib/types";

  interface Props {
    config: McpServerConfigInput;
  }

  let { config }: Props = $props();

  let testResult = $state<McpConnectionTestResultView | null>(null);
  let testError = $state<string | null>(null);
  let testing = $state(false);

  async function runTest(): Promise<void> {
    testing = true;
    testResult = null;
    testError = null;
    try {
      testResult = await invoke<McpConnectionTestResultView>("test_mcp_connection", { config });
    } catch (err: unknown) {
      testError = err instanceof Error ? err.message : String(err);
    } finally {
      testing = false;
    }
  }
</script>

<div class="space-y-4" data-testid="wizard-step-test">
  <p class="text-sm text-muted-foreground">{$t("integrations.wizard.test_hint")}</p>

  <Button
    size="sm"
    onclick={runTest}
    disabled={testing}
    data-testid="test-connection-btn"
  >
    {#if testing}
      <Spinner size={14} class="mr-2" />
      {$t("integrations.wizard.test_testing")}
    {:else}
      {$t("integrations.wizard.test_button")}
    {/if}
  </Button>

  {#if testResult}
    <div
      class="rounded-md border border-emerald-200 bg-emerald-50 px-4 py-3 dark:border-emerald-800 dark:bg-emerald-950/30"
      data-testid="test-result-success"
    >
      <div class="flex items-center gap-2">
        <CheckCircle2 size={16} class="shrink-0 text-emerald-600 dark:text-emerald-400" />
        <p class="text-sm font-medium text-emerald-700 dark:text-emerald-300">
          {$t("integrations.wizard.test_success", { values: { count: testResult.tools.length } })}
        </p>
      </div>
      <p class="mt-1 text-xs text-emerald-600/80 dark:text-emerald-400/80">
        {$t("integrations.wizard.test_duration", {
          values: { ms: testResult.test_duration_ms },
        })}
      </p>
    </div>
  {/if}

  {#if testError}
    <div
      class="rounded-md border border-destructive/30 bg-destructive/5 px-4 py-3"
      data-testid="test-result-error"
    >
      <div class="flex items-center gap-2">
        <XCircle size={16} class="shrink-0 text-destructive" />
        <p class="text-sm font-medium text-destructive">
          {$t("integrations.wizard.test_error_title")}
        </p>
      </div>
      <p class="mt-1 font-mono text-xs text-destructive/80">{testError}</p>
    </div>
  {/if}
</div>
