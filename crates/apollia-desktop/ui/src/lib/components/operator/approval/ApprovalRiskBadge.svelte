<script lang="ts">
  /**
   * Approval risk badge. Thin wrapper over the canonical RiskBadge with
   * `kind="approval"`, which preserves the approval i18n keys
   * (`approvals.risk.*`), `data-testid="approval-risk-badge"` and
   * `data-risk-level`. The level -> icon/color/label mapping lives once in
   * RiskBadge; this wrapper exists only to keep the approval-specific public
   * API and call-sites unchanged.
   */
  import RiskBadge from "../badges/RiskBadge.svelte";

  export type ApprovalRiskLevel = "low" | "medium" | "high" | "critical";

  interface Props {
    level: ApprovalRiskLevel;
    compact?: boolean;
  }

  let { level, compact = false }: Props = $props();
</script>

<RiskBadge {level} {compact} kind="approval" />
