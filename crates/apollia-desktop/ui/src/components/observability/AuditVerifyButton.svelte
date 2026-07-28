<script lang="ts" module>
  import type { AuditVerifyResult as VerifyResult } from "$lib/ipc/audit";

  /** Mutable verify state shared between the component and its orchestrator. */
  export interface VerifyState {
    loading: boolean;
    result: VerifyResult | null;
    error: string | null;
  }

  /** Normalizes an unknown thrown value into a display string. */
  export function verifyErrorMessage(err: unknown): string {
    return err instanceof Error ? err.message : String(err);
  }

  /**
   * Runs `verify` against `state`, enforcing single-flight.
   *
   * A call while `state.loading` is already true returns immediately without
   * issuing a second request (double-click protection). On success the verdict
   * lands in `state.result`; on failure the normalized message lands in
   * `state.error`. `state.loading` is always cleared at the end.
   */
  export async function performVerify(
    state: VerifyState,
    verify: () => Promise<VerifyResult>,
  ): Promise<void> {
    if (state.loading) return;
    state.loading = true;
    state.result = null;
    state.error = null;
    try {
      state.result = await verify();
    } catch (err: unknown) {
      state.error = verifyErrorMessage(err);
    } finally {
      state.loading = false;
    }
  }
</script>

<script lang="ts">
  /**
   * `AuditVerifyButton` - triggers a hash-chain integrity check for a run.
   *
   * Delegates the IPC call to `lib/ipc/audit` and renders the verdict through
   * `AuditVerifyResult`. Single-flight: a second click during loading is a
   * no-op. Builder-only surface.
   */
  import { t } from "svelte-i18n";
  import { ShieldCheck } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { verifyAuditRun } from "$lib/ipc/audit";
  import AuditVerifyResult from "./AuditVerifyResult.svelte";

  interface Props {
    runId: string;
  }

  let { runId }: Props = $props();

  let state = $state<VerifyState>({ loading: false, result: null, error: null });

  async function handleVerify(): Promise<void> {
    await performVerify(state, () => verifyAuditRun(runId));
  }
</script>

<div class="flex flex-col items-start gap-1">
  <Button
    variant="outline"
    size="sm"
    disabled={state.loading}
    onclick={() => void handleVerify()}
    data-testid="verify-run-{runId}"
  >
    <ShieldCheck class="mr-1.5 h-3.5 w-3.5" />
    {$t("observability.verify_run")}
  </Button>

  {#if state.loading}
    <span class="text-body-xs text-muted-foreground" data-testid="verify-loading">
      {$t("observability.verify_loading")}
    </span>
  {:else if state.error}
    <span class="text-body-xs text-destructive" data-testid="verify-error">
      {$t("observability.verify_error", { values: { message: state.error } })}
    </span>
  {:else if state.result}
    <AuditVerifyResult result={state.result} {runId} />
  {/if}
</div>
