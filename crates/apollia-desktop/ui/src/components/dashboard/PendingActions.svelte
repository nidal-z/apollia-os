<script lang="ts">
  import { t } from "svelte-i18n";
  import { currentRoute } from "$lib/stores/navigation";
  import { Card, CardContent } from "$lib/components/ui/card";
  import { Button } from "$lib/components/ui/button";
  import { ShieldCheck } from "lucide-svelte";

  interface Props {
    count: number;
  }

  let { count }: Props = $props();

  function navigateToApprovals() {
    currentRoute.set("approvals");
  }
</script>

<Card class="border-[var(--apollia-warning)] bg-[var(--apollia-warning)]/5" data-testid="pending-actions">
  <CardContent class="flex items-center gap-4 py-4">
    <ShieldCheck size={24} class="shrink-0 text-[var(--apollia-warning)]" />
    <div class="flex-1">
      <p class="text-sm font-semibold">
        {$t('dashboard.approvals_waiting', { values: { count } })}
      </p>
      <p class="text-xs text-muted-foreground">{$t('dashboard.approvals_subtitle')}</p>
    </div>
    <Button size="sm" variant="outline" onclick={navigateToApprovals} data-testid="pending-actions-review-btn">
      {$t('dashboard.review')}
    </Button>
  </CardContent>
</Card>
