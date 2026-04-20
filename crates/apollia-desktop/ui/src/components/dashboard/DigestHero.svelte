<script lang="ts">
  import { t, locale } from "svelte-i18n";
  import { Card } from "$lib/components/ui/card";
  import DigestNarration from "./DigestNarration.svelte";
  import type { DailyDigest } from "$lib/stores/dailyDigest";

  interface Props {
    digest: DailyDigest | null;
    weekTasksCount: number;
    weekPendingCount: number;
    loading: boolean;
    onrefresh: () => void;
  }

  let { digest, weekTasksCount, weekPendingCount, loading, onrefresh }: Props = $props();

  const humanizedDate = $derived.by(() => {
    const loc = ($locale ?? "fr").startsWith("fr") ? "fr-FR" : "en-US";
    return new Intl.DateTimeFormat(loc, {
      weekday: "long",
      day: "numeric",
      month: "long",
    }).format(new Date());
  });
</script>

<Card
  premium
  class="relative overflow-hidden px-5 py-5"
  data-testid="digest-hero"
>
  <div class="pointer-events-none absolute inset-0 bg-gradient-accent opacity-70" aria-hidden="true"></div>
  <div class="relative flex flex-col gap-3">
    <div class="flex items-start justify-between gap-4">
      <div>
        <h1 class="text-display-lg text-foreground">
          {$t("dashboard.digest_hero_title", { values: { date: humanizedDate } })}
        </h1>
        <p class="mt-1 text-xs uppercase tracking-wider text-muted-foreground" data-testid="digest-hero-weekly">
          {$t("dashboard.week_tasks_count", { values: { n: weekTasksCount } })}
          {#if weekPendingCount > 0}
            · <span class="text-warning">{$t("dashboard.week_pending_suffix", { values: { m: weekPendingCount } })}</span>
          {/if}
        </p>
      </div>
    </div>

    <DigestNarration {digest} {loading} {onrefresh} />
  </div>
</Card>
