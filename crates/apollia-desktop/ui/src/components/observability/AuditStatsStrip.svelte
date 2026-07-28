<script lang="ts">
  // KPI strip for the audit trail (entries, tools, failures, average duration).
  // Extracted verbatim from AuditTrailTable so the table stays focused on data
  // orchestration. The parent computes `stats`; this strip only renders it.
  import { t } from "svelte-i18n";

  interface AuditStats {
    entries: number;
    tools: number;
    failures: number;
    avgMs: number;
  }

  interface Props {
    stats: AuditStats;
  }

  let { stats }: Props = $props();

  function formatDuration(ms: number | null): string {
    if (ms === null || ms === undefined) return "-";
    if (ms < 1000) return `${ms}ms`;
    return `${(ms / 1000).toFixed(1)}s`;
  }
</script>

<div class="grid grid-cols-2 md:grid-cols-4 gap-3" data-testid="audit-stats">
  <article class="glass-inset rounded-lg px-4 py-3">
    <div class="section-meta mb-1.5">
      {$t('observability.audit_kpi_entries')}
    </div>
    <div class="text-heading-lg font-semibold tabular-nums leading-none">{stats.entries}</div>
  </article>
  <article class="glass-inset rounded-lg px-4 py-3">
    <div class="section-meta mb-1.5">
      {$t('observability.audit_kpi_tools')}
    </div>
    <div class="text-heading-lg font-semibold tabular-nums leading-none">{stats.tools}</div>
  </article>
  <article class="glass-inset rounded-lg px-4 py-3">
    <div class="section-meta mb-1.5">
      {$t('observability.audit_kpi_failures')}
    </div>
    <div
      class="text-heading-lg font-semibold tabular-nums leading-none"
      class:text-destructive={stats.failures > 0}
    >
      {stats.failures}
    </div>
  </article>
  <article class="glass-inset rounded-lg px-4 py-3">
    <div class="section-meta mb-1.5">
      {$t('observability.audit_kpi_avg_duration')}
    </div>
    <div class="text-heading-lg font-semibold tabular-nums leading-none">
      {stats.avgMs > 0 ? formatDuration(Math.round(stats.avgMs)) : "-"}
    </div>
  </article>
</div>
