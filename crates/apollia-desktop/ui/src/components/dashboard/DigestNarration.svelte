<script lang="ts">
  import { t } from "svelte-i18n";
  import { RefreshCw, Sparkles, ChevronDown, ChevronUp } from "lucide-svelte";
  import type { DailyDigest } from "$lib/stores/dailyDigest";

  interface Props {
    digest: DailyDigest | null;
    loading: boolean;
    onrefresh: () => void;
  }

  let { digest, loading, onrefresh }: Props = $props();

  let expanded = $state(false);
  let showFacts = $state(false);

  const narration = $derived(digest?.narration ?? "");

  // Mobile tronque à ~3 lignes, "Voir plus" pour tout afficher.
  const truncated = $derived(narration.length > 180 && !expanded);
  const displayed = $derived(truncated ? narration.slice(0, 180).trimEnd() + "…" : narration);
</script>

<div class="rounded-md bg-card/70 p-3" data-testid="digest-narration">
  <div class="flex items-start gap-2">
    <Sparkles size={14} class="mt-0.5 shrink-0 text-primary/70" />
    <div class="flex-1 min-w-0">
      {#if loading && !digest}
        <p class="animate-pulse text-sm text-muted-foreground">{$t("dashboard.digest_loading")}</p>
      {:else if !digest}
        <p class="text-sm text-muted-foreground">{$t("dashboard.digest_empty")}</p>
      {:else}
        <p class="text-sm leading-relaxed text-foreground" data-testid="digest-narration-text">
          {displayed}
        </p>
        {#if narration.length > 180}
          <button
            class="mt-1 text-[11px] text-primary hover:text-primary/80 md:hidden"
            onclick={() => (expanded = !expanded)}
          >
            {expanded ? $t("common.see_less") : $t("common.see_more")}
          </button>
        {/if}
      {/if}
    </div>

    <button
      type="button"
      class="shrink-0 inline-flex items-center gap-1 rounded-md border border-border/50 bg-card px-2 py-1 text-[11px] text-muted-foreground transition-colors hover:bg-muted/60 disabled:opacity-50"
      disabled={loading}
      onclick={onrefresh}
      data-testid="digest-refresh"
      title={$t("dashboard.digest_refresh_tooltip")}
    >
      <RefreshCw size={11} class={loading ? "animate-spin" : ""} />
      <span>{$t("dashboard.digest_refresh")}</span>
    </button>
  </div>

  {#if digest}
    <button
      class="mt-2 inline-flex items-center gap-1 text-[10px] uppercase tracking-wider text-muted-foreground/80 hover:text-muted-foreground"
      onclick={() => (showFacts = !showFacts)}
      data-testid="digest-facts-toggle"
    >
      {#if showFacts}
        <ChevronUp size={10} />
      {:else}
        <ChevronDown size={10} />
      {/if}
      {$t("dashboard.digest_why_this")}
    </button>
    {#if showFacts}
      <dl class="mt-2 grid grid-cols-2 gap-1 text-[11px] text-muted-foreground sm:grid-cols-3">
        <div><dt class="inline">{$t("dashboard.fact_completed")}</dt> <dd class="inline font-medium text-foreground">{digest.sourceFacts.tasksCompleted}</dd></div>
        <div><dt class="inline">{$t("dashboard.fact_failed")}</dt> <dd class="inline font-medium text-foreground">{digest.sourceFacts.tasksFailed}</dd></div>
        <div><dt class="inline">{$t("dashboard.fact_pending")}</dt> <dd class="inline font-medium text-foreground">{digest.sourceFacts.approvalsPending}</dd></div>
        <div><dt class="inline">{$t("dashboard.fact_insights")}</dt> <dd class="inline font-medium text-foreground">{digest.sourceFacts.insightsCreated}</dd></div>
        <div><dt class="inline">{$t("dashboard.fact_automations_failed")}</dt> <dd class="inline font-medium text-foreground">{digest.sourceFacts.automationsFailed}</dd></div>
      </dl>
    {/if}
  {/if}
</div>
