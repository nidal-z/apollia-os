<script lang="ts">
  import { t } from "svelte-i18n";
  import { AlertTriangle, Shield, ShieldAlert, Zap } from "lucide-svelte";

  export type ApprovalRiskLevel = "low" | "medium" | "high" | "critical";

  interface Props {
    level: ApprovalRiskLevel;
    compact?: boolean;
  }

  let { level, compact = false }: Props = $props();

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
        return $t("approvals.risk.critical");
      case "high":
        return $t("approvals.risk.high");
      case "medium":
        return $t("approvals.risk.medium");
      default:
        return $t("approvals.risk.low");
    }
  });
</script>

<span
  class="inline-flex items-center gap-1 rounded-md border px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-wide {badgeClass}"
  data-testid="approval-risk-badge"
  data-risk-level={level}
  aria-label={label}
>
  <Icon class="h-3 w-3" aria-hidden="true" />
  {#if !compact}
    <span>{label}</span>
  {/if}
</span>
