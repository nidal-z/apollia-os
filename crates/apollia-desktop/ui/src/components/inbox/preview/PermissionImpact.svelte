<script lang="ts">
  /**
   * Compact impact / consequences panel.
   *
   * Surfaces `estimated_impact`, `consequence_if_approved`, and
   * `consequence_if_refused` from the permission payload. When those
   * fields are missing, the parent is expected to backfill them via the
   * Meta-LLM `GenerateConsequenceExplanation` route — this component is
   * purely presentational.
   */
  import { t } from "svelte-i18n";
  import { Check, X } from "lucide-svelte";
  import RiskBadge from "../../hitl/RiskBadge.svelte";
  import type { PermissionImpactPayload } from "./types";

  interface Props {
    payload: PermissionImpactPayload;
  }

  let { payload }: Props = $props();

  const impactLine = $derived.by(() => {
    if (payload.summary) return payload.summary;
    if (payload.resource && payload.action) {
      const mag = payload.magnitude ? ` (${payload.magnitude})` : "";
      return `${payload.action} — ${payload.resource}${mag}`;
    }
    return null;
  });
</script>

<div class="space-y-3 rounded-md border border-border bg-card/40 p-3" data-testid="permission-impact">
  {#if payload.risk_level}
    <div class="flex items-center gap-2">
      <RiskBadge level={payload.risk_level} />
      {#if payload.risk_reasons?.length}
        <span class="text-[11px] text-muted-foreground">{payload.risk_reasons.join(" · ")}</span>
      {/if}
    </div>
  {/if}

  {#if impactLine}
    <section>
      <h4 class="mb-0.5 text-[11px] uppercase tracking-wide text-muted-foreground">
        {$t("permissions.impact.label")}
      </h4>
      <p class="text-[13px]">{impactLine}</p>
    </section>
  {/if}

  {#if payload.consequence_if_approved || payload.consequence_if_refused}
    <section class="grid gap-2 sm:grid-cols-2">
      {#if payload.consequence_if_approved}
        <div class="flex items-start gap-2 rounded border border-success/30 bg-success/5 px-2 py-1.5 text-[12px]">
          <Check size={14} class="mt-0.5 shrink-0 text-success" />
          <div>
            <p class="font-medium text-success">{$t("permissions.impact.if_approved")}</p>
            <p class="text-muted-foreground">{payload.consequence_if_approved}</p>
          </div>
        </div>
      {/if}
      {#if payload.consequence_if_refused}
        <div class="flex items-start gap-2 rounded border border-border px-2 py-1.5 text-[12px]">
          <X size={14} class="mt-0.5 shrink-0 text-muted-foreground" />
          <div>
            <p class="font-medium">{$t("permissions.impact.if_refused")}</p>
            <p class="text-muted-foreground">{payload.consequence_if_refused}</p>
          </div>
        </div>
      {/if}
    </section>
  {/if}
</div>
