<script lang="ts" module>
  import type { AuditVerifyResult } from "$lib/ipc/audit";

  /** Verdict kind driving the icon, token color and testid of the result. */
  export type VerifyVerdict = "ok" | "fail";

  /** Maps an integrity result to its verdict kind. */
  export function verifyVerdict(result: AuditVerifyResult): VerifyVerdict {
    return result.ok ? "ok" : "fail";
  }
</script>

<script lang="ts">
  /**
   * `AuditVerifyResult` - renders the integrity verdict from `verifyAuditRun`.
   *
   * Intact: a success line. Corrupted: a destructive line plus the broken link
   * identifier and the runtime message. Builder-only surface.
   */
  import { t } from "svelte-i18n";
  import { CheckCircle2, XCircle } from "lucide-svelte";

  interface Props {
    result: AuditVerifyResult;
    runId: string;
  }

  let { result, runId }: Props = $props();
</script>

<div class="mt-3 flex flex-col gap-2" data-testid={`verify-result-${runId}`}>
  {#if verifyVerdict(result) === "ok"}
    <span
      class="inline-flex items-center gap-2 text-success"
      data-testid="verify-verdict-ok"
    >
      <CheckCircle2 class="h-4 w-4" />
      <span class="text-[13px] font-medium">{$t("observability.verify_ok")}</span>
    </span>
  {:else}
    <span
      class="inline-flex items-center gap-2 text-destructive"
      data-testid="verify-verdict-fail"
    >
      <XCircle class="h-4 w-4" />
      <span class="text-[13px] font-medium">{$t("observability.verify_fail")}</span>
    </span>
    {#if result.broken_at}
      <p class="text-[12px] text-destructive/80" data-testid="verify-broken-at">
        {$t("observability.verify_broken_at", { values: { entry: result.broken_at } })}
      </p>
    {/if}
    {#if result.message}
      <p class="text-[12px] text-muted-foreground">{result.message}</p>
    {/if}
  {/if}
</div>
