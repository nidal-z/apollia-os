<script lang="ts">
  import { t } from "svelte-i18n";
  import { ShieldAlert, ShieldCheck } from "lucide-svelte";
  import type { HallucinationRisk } from "$lib/types";

  interface Props {
    /** Score agrégé + facteurs (US-SP42-048, toujours-on). */
    risk: HallucinationRisk;
  }

  let { risk }: Props = $props();

  // 0-24 = safe, 25-49 = low, 50-74 = elevated, 75-100 = high.
  const band = $derived(
    risk.score >= 75
      ? "high"
      : risk.score >= 50
      ? "elevated"
      : risk.score >= 25
      ? "low"
      : "safe",
  );

  const tooltip = $derived(
    risk.factors.length > 0
      ? risk.factors.join(" · ")
      : $t("session_meta.risk.none", {
          default: "No hallucination signals detected in this session.",
        }),
  );
</script>

<span
  class="risk-badge band-{band}"
  title={tooltip}
  data-testid="hallucination-risk-badge"
  data-band={band}
  role="status"
>
  {#if band === "safe"}
    <ShieldCheck size={12} />
  {:else}
    <ShieldAlert size={12} />
  {/if}
  <span class="label">
    {$t("session_meta.risk.label", { default: "Hallucination risk" })}
    <strong>{risk.score}/100</strong>
  </span>
</span>

<style>
  .risk-badge {
    display: inline-flex;
    align-items: center;
    gap: 0.375rem;
    padding: 0.125rem 0.5rem;
    border-radius: 9999px;
    border: 1px solid transparent;
    font-size: 11px;
    font-weight: 600;
    line-height: 1.4;
    cursor: help;
  }
  .band-safe {
    border-color: hsl(var(--success) / 0.4);
    background-color: hsl(var(--success) / 0.1);
    color: hsl(var(--success));
  }
  .band-low {
    border-color: hsl(var(--muted-foreground) / 0.3);
    background-color: hsl(var(--muted) / 0.3);
    color: hsl(var(--muted-foreground));
  }
  .band-elevated {
    border-color: hsl(var(--warning) / 0.5);
    background-color: hsl(var(--warning) / 0.12);
    color: hsl(var(--warning));
  }
  .band-high {
    border-color: hsl(var(--destructive) / 0.55);
    background-color: hsl(var(--destructive) / 0.12);
    color: hsl(var(--destructive));
  }
  .label {
    white-space: nowrap;
  }
  strong {
    font-variant-numeric: tabular-nums;
  }
</style>
