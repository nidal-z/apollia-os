<script lang="ts">
  import { t } from "svelte-i18n";
  import { CheckCircle2, XCircle, RefreshCw, ArrowLeft } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { Spinner } from "$lib/components/ui/progress";
  import type {
    McpConnectionTestResultView,
    McpServerConfigInput,
  } from "$lib/types";
  import { testMcpConnection } from "$lib/ipc/connections";

  interface Props {
    config: McpServerConfigInput;
    /** When true, a "Skip test (expert)" escape hatch is shown (builder only). */
    allowBypass?: boolean;
    /** Called when the test succeeds - parent uses this to unlock the Next button. */
    onsuccess?: (result: McpConnectionTestResultView) => void;
    /** Called when user asks to go back to the Auth step to fix credentials. */
    onfixauth?: () => void;
    /** Called when expert user chooses to bypass the test (builder only). */
    onbypass?: () => void;
  }

  let {
    config,
    allowBypass = false,
    onsuccess = () => {},
    onfixauth = () => {},
    onbypass = () => {},
  }: Props = $props();

  let testResult = $state<McpConnectionTestResultView | null>(null);
  let testError = $state<string | null>(null);
  let testing = $state(false);
  let attemptCount = $state(0);

  /// Humanise a raw backend error message for operator users.
  /// The backend exposes an `error_analysis.user_friendly` field on some errors;
  /// if absent we fall back to pattern matching common MCP errors.
  function humanizeError(raw: string): string {
    const lower = raw.toLowerCase();
    if (lower.includes("401") || lower.includes("unauthorized")) {
      return $t("integrations.wizard.test_err.unauthorized");
    }
    if (lower.includes("403") || lower.includes("forbidden")) {
      return $t("integrations.wizard.test_err.forbidden");
    }
    if (lower.includes("404") || lower.includes("not found")) {
      return $t("integrations.wizard.test_err.not_found");
    }
    if (
      lower.includes("timeout") ||
      lower.includes("timed out") ||
      lower.includes("initialize timeout")
    ) {
      // Initialize timeout = the server started but never replied to the MCP
      // handshake. Typically a wrong endpoint, slow npx download on cold
      // cache, or auth that takes too long.
      return $t("integrations.wizard.test_err.handshake_timeout");
    }
    if (lower.includes("http 401") || lower.includes("transport closed: http 401")) {
      // The remote returned 401 Unauthorized at the transport layer (before
      // any MCP handshake). Typical for OAuth-only endpoints (Figma cloud,
      // Notion, Linear) where a Bearer token isn't accepted.
      return $t("integrations.wizard.test_err.unauthorized");
    }
    if (lower.includes("http 403") || lower.includes("transport closed: http 403")) {
      return $t("integrations.wizard.test_err.forbidden");
    }
    if (lower.includes("http 404") || lower.includes("transport closed: http 404")) {
      return $t("integrations.wizard.test_err.not_found");
    }
    if (
      lower.includes("transport closed") ||
      lower.includes("transport send channel closed") ||
      lower.includes("server exited") ||
      lower.includes("stdin closed")
    ) {
      // The subprocess (stdio) or the upstream (HTTP/SSE) closed the
      // connection before the handshake completed. Most common causes:
      // - npm package not found ("npx <pkg>" fails to download)
      // - HTTP endpoint unreachable (wrong URL, DNS down)
      // - auth rejected at connection layer (rare - usually surfaced as 401)
      return $t("integrations.wizard.test_err.transport_closed");
    }
    if (lower.includes("network") || lower.includes("dns") || lower.includes("connect")) {
      return $t("integrations.wizard.test_err.network");
    }
    if (lower.includes("enoent") || lower.includes("command not found")) {
      return $t("integrations.wizard.test_err.command_missing");
    }
    return $t("integrations.wizard.test_err.generic");
  }

  async function runTest(): Promise<void> {
    testing = true;
    testResult = null;
    testError = null;
    attemptCount += 1;
    try {
      const response = await testMcpConnection(config);
      if (response.kind === "success") {
        // Strip the discriminant - `onsuccess` consumers expect the legacy
        // success shape verbatim.
        const { kind: _kind, ...result } = response;
        void _kind;
        testResult = result;
        onsuccess(result);
      } else {
        // oauth_required at test time means the static-token path failed
        // authentication. An inline "Sign in" CTA is not built yet; for
        // now report it as an error directing the user back to the Auth step.
        testError = $t("integrations.wizard.test_err.oauth_required");
      }
    } catch (err: unknown) {
      testError = err instanceof Error ? err.message : String(err);
    } finally {
      testing = false;
    }
  }
</script>

<div class="space-y-4" data-testid="wizard-step-test">
  <p class="text-sm text-muted-foreground">
    {$t("integrations.wizard.test_hint")}
  </p>

  {#if !testResult}
    <Button
      size="sm"
      onclick={runTest}
      disabled={testing}
      data-testid="test-connection-btn"
    >
      {#if testing}
        <Spinner size={14} class="mr-2" />
        {$t("integrations.wizard.test_testing")}
      {:else if attemptCount === 0}
        {$t("integrations.wizard.test_button")}
      {:else}
        <RefreshCw size={14} class="mr-2" aria-hidden="true" />
        {$t("integrations.wizard.test_retry")}
      {/if}
    </Button>
  {/if}

  {#if testResult}
    <div
      class="rounded-md border border-success/30 bg-success/10 px-4 py-3"
      data-testid="test-result-success"
    >
      <div class="flex items-center gap-2">
        <CheckCircle2
          size={16}
          class="shrink-0 text-success"
          aria-hidden="true"
        />
        <p
          class="text-sm font-medium text-success-a11y"
        >
          {$t("integrations.wizard.test_success", {
            values: { count: testResult.tools.length },
          })}
        </p>
      </div>
      <p
        class="mt-1 text-xs text-success/80"
      >
        {$t("integrations.wizard.test_duration", {
          values: { ms: testResult.test_duration_ms },
        })}
      </p>
    </div>
  {/if}

  {#if testError && !testResult}
    <div
      class="rounded-md border border-destructive/30 bg-destructive/5 px-4 py-3 space-y-2"
      data-testid="test-result-error"
    >
      <div class="flex items-center gap-2">
        <XCircle
          size={16}
          class="shrink-0 text-destructive"
          aria-hidden="true"
        />
        <p class="text-sm font-medium text-destructive">
          {humanizeError(testError)}
        </p>
      </div>
      <details class="text-xs">
        <summary class="cursor-pointer text-muted-foreground">
          {$t("integrations.wizard.test_err.raw_details")}
        </summary>
        <p class="mt-1 font-mono text-caption text-destructive/80 break-all">
          {testError}
        </p>
      </details>

      <div class="flex flex-wrap gap-2 pt-1">
        <Button
          size="sm"
          variant="outline"
          onclick={onfixauth}
          data-testid="test-fix-auth-btn"
        >
          <ArrowLeft size={12} class="mr-1" aria-hidden="true" />
          {$t("integrations.wizard.test_err.fix_auth")}
        </Button>
        <Button
          size="sm"
          variant="outline"
          onclick={runTest}
          disabled={testing}
          data-testid="test-retry-btn"
        >
          <RefreshCw size={12} class="mr-1" aria-hidden="true" />
          {$t("integrations.wizard.test_retry")}
        </Button>
        {#if allowBypass}
          <Button
            size="sm"
            variant="outline"
            onclick={onbypass}
            data-testid="test-bypass-btn"
          >
            {$t("integrations.wizard.test_err.bypass")}
          </Button>
        {/if}
      </div>
    </div>
  {/if}
</div>
