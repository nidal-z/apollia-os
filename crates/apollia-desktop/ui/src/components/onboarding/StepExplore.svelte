<script lang="ts">
  import { t } from "svelte-i18n";
  import { Button } from "$lib/components/ui/button";
  import { Bot, ListChecks, Brain, Timer, Activity } from "lucide-svelte";
  import type { ComponentType } from "svelte";

  interface Props {
    onFinish: () => void;
  }

  let { onFinish }: Props = $props();

  interface ExploreLink {
    route: string;
    labelKey: string;
    descKey: string;
    icon: ComponentType;
  }

  const LINKS: ExploreLink[] = [
    { route: "agents", labelKey: "nav.agents", descKey: "onboarding.explore_agents_desc", icon: Bot },
    { route: "tasks", labelKey: "nav.tasks", descKey: "onboarding.explore_tasks_desc", icon: ListChecks },
    { route: "llm", labelKey: "nav.llm", descKey: "onboarding.explore_llm_desc", icon: Brain },
    { route: "triggers", labelKey: "nav.triggers", descKey: "onboarding.explore_triggers_desc", icon: Timer },
    { route: "observability", labelKey: "nav.observability", descKey: "onboarding.explore_observability_desc", icon: Activity },
  ];
</script>

<div class="space-y-6" data-testid="step-explore">
  <div>
    <h2 class="text-xl font-semibold">{$t('onboarding.explore_title')}</h2>
    <p class="mt-1 text-sm text-muted-foreground">
      {$t('onboarding.explore_subtitle')}
    </p>
  </div>

  <div class="grid gap-3">
    {#each LINKS as link (link.route)}
      {@const Icon = link.icon}
      <div class="flex items-center gap-4 rounded-lg border bg-card p-4">
        <span class="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-primary/10 text-primary">
          <Icon size={18} />
        </span>
        <div class="flex-1 min-w-0">
          <p class="text-sm font-semibold">{$t(link.labelKey)}</p>
          <p class="text-xs text-muted-foreground">{$t(link.descKey)}</p>
        </div>
      </div>
    {/each}
  </div>

  <div class="flex justify-end">
    <Button onclick={onFinish}>{$t('onboarding.explore_done')}</Button>
  </div>
</div>
