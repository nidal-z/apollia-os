<script lang="ts" module>
  /**
   * Generic risk badge surfaced on HITL cards.
   *
   * Maps an `ImpactLevel` to an icon + color. Kept separate from
   * `ApprovalRiskBadge` so that non-approval HITL surfaces (AskUser,
   * memory write) can reuse a single component without importing the
   * approval-specific i18n keys.
   */
  export type ImpactLevel = "low" | "medium" | "high" | "critical";
</script>

<script lang="ts">
  import { t } from "svelte-i18n";
  import { AlertTriangle, Shield, ShieldAlert, Zap } from "lucide-svelte";

  interface Props {
    level: ImpactLevel;
    /** Optional numeric severity (0-10) to surface next to the label. */
    severity?: number;
    compact?: boolean;
  }

  let { level, severity, compact = false }: Props = $props();

  const badgeClass = $derived.by(() => {
    switch (level) {
      case "critical":
        return "bg-destructive/10 text-destructive border-destructive/40";
      case "high":
        return "bg-orange-500/10 text-orange-500 border-orange-500/30";
      case "medium":
        return "bg-warning/10 text-warning border-warning/30";
      default:
        return "bg-success/10 text-success border-success/30";
    }
  });

  const Icon = $derived.by(() => {
    switch (level) {
      case "critical":
        return Zap;
      case "high":
        return ShieldAlert;
      case "medium":
        return AlertTriangle;
      default:
        return Shield;
    }
  });

  const label = $derived.by(() => {
    switch (level) {
      case "critical":
        return $t("hitl.impact.critical");
      case "high":
        return $t("hitl.impact.high");
      case "medium":
        return $t("hitl.impact.medium");
      default:
        return $t("hitl.impact.low");
    }
  });
</script>

<span
  class="inline-flex items-center gap-1 rounded-md border px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-wide {badgeClass}"
  data-testid="risk-badge"
  data-impact-level={level}
  aria-label={label}
>
  <Icon class="h-3 w-3" aria-hidden="true" />
  {#if !compact}
    <span>{label}</span>
    {#if severity !== undefined}
      <span class="font-mono opacity-70">{severity}/10</span>
    {/if}
  {/if}
</span>
