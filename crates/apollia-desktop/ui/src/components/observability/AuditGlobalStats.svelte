<script lang="ts">
  /**
   * `AuditGlobalStats` - journal-wide counters for the audit trail.
   *
   * Reads `get_audit_stats`, which aggregates the whole trail server-side. This
   * is deliberately distinct from `AuditStatsStrip`, which counts only the rows
   * the table has loaded: one answers "what does the journal hold", the other
   * "what am I looking at right now".
   */
  import { t } from "svelte-i18n";
  import { Archive } from "lucide-svelte";
  import { Card } from "$lib/components/ui/card";
  import { getAuditStats, type AuditStatsSummary } from "$lib/ipc/observability";

  interface Props {
    /**
     * Counter the parent bumps once an action that concerns the journal has
     * succeeded, an integrity verification for instance. Each new value
     * triggers a fresh read, so the totals never stay on what they were when
     * the tab was opened.
     */
    refreshToken?: number;
  }

  let { refreshToken = 0 }: Props = $props();

  let stats = $state<AuditStatsSummary | null>(null);
  let errorMessage = $state<string | null>(null);
  /** Token value the displayed counters were read for. */
  let loadedToken = -1;

  async function load(): Promise<void> {
    try {
      stats = await getAuditStats();
      errorMessage = null;
    } catch (err: unknown) {
      // The totals are a complement to the table, never a precondition: a
      // failure degrades to one caption instead of hiding the trail.
      stats = null;
      errorMessage = err instanceof Error ? err.message : String(err);
    }
  }

  // First read on mount, then one read per bump of the token.
  $effect(() => {
    if (loadedToken === refreshToken) return;
    loadedToken = refreshToken;
    void load();
  });
</script>

<Card class="px-4 py-3" data-testid="audit-global-stats">
  <div class="mb-2.5 flex items-center gap-2">
    <Archive class="h-3.5 w-3.5 text-muted-foreground/60" aria-hidden="true" />
    <span class="section-meta">{$t("observability.audit_global_title")}</span>
    <span class="text-caption text-muted-foreground/70">
      {$t("observability.audit_global_hint")}
    </span>
  </div>

  {#if errorMessage !== null}
    <p class="text-caption text-muted-foreground" data-testid="audit-global-error">
      {$t("observability.audit_global_error", { values: { message: errorMessage } })}
    </p>
  {:else if stats !== null}
    <dl class="grid grid-cols-1 gap-3 sm:grid-cols-3">
      <div class="glass-inset rounded-lg px-3 py-2" data-testid="audit-global-events">
        <dt class="section-meta mb-1">{$t("observability.audit_global_events")}</dt>
        <dd class="text-heading-lg font-semibold leading-none tabular-nums">
          {stats.total_events}
        </dd>
      </div>
      <div class="glass-inset rounded-lg px-3 py-2" data-testid="audit-global-tools">
        <dt class="section-meta mb-1">{$t("observability.audit_global_tools")}</dt>
        <dd class="text-heading-lg font-semibold leading-none tabular-nums">
          {stats.unique_tools}
        </dd>
      </div>
      <div class="glass-inset rounded-lg px-3 py-2" data-testid="audit-global-agents">
        <dt class="section-meta mb-1">{$t("observability.audit_global_agents")}</dt>
        <dd class="text-heading-lg font-semibold leading-none tabular-nums">
          {stats.unique_agents}
        </dd>
      </div>
    </dl>
  {:else}
    <p class="text-caption text-muted-foreground">{$t("common.loading")}</p>
  {/if}
</Card>
