<script lang="ts">
  /**
   * The head of the Guide tab: where the user is, what the screen is for, and
   * what the guide offers to do from here. The offered action stands out as a
   * card; the lesser ones are rows. Each opens a screen and nothing else.
   */
  import { t } from "svelte-i18n";
  import { ChevronRight } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { navigateTo } from "$lib/stores/navigation";
  import type { GuideScreenView } from "$lib/apolliaGuide/screenContexts";

  interface Props {
    view: GuideScreenView;
  }

  let { view }: Props = $props();
</script>

<section
  class="flex flex-col gap-3 px-4 pb-3 pt-4"
  data-testid="guide-context"
  data-context={view.id}
>
  <div class="flex flex-col gap-1">
    <p class="text-caption text-muted-foreground" data-testid="guide-context-trail">
      {$t("guide.you_are_on")}
      {view.trail.join(" › ")}
    </p>
    <h2 class="text-heading-sm text-foreground">{view.title}</h2>
    <p class="text-body-sm text-muted-foreground" data-testid="guide-context-text">
      {view.text}
    </p>
  </div>

  {#if view.action || view.secondary.length > 0}
    <div class="flex flex-col gap-2">
      <h3 class="text-overline uppercase text-muted-foreground">
        {$t("guide.actions_title")}
      </h3>
      {#if view.action}
        {@const action = view.action}
        <div class="flex flex-col gap-2 rounded-lg border border-border bg-surface-2 p-3">
          <p class="text-body-sm text-foreground">{action.result}</p>
          <Button
            variant="primary-solid"
            size="sm"
            class="self-start"
            onclick={() => navigateTo(action.route)}
            data-testid="guide-action"
            data-route={action.route}
          >
            {action.label}
          </Button>
        </div>
      {/if}
      {#each view.secondary as action (action.route)}
        <button
          type="button"
          class="flex items-center justify-between gap-2 rounded-md px-2 py-1.5 text-left
            text-body-sm text-foreground transition-colors hover:bg-muted
            focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/50
            motion-reduce:transition-none"
          onclick={() => navigateTo(action.route)}
          data-testid="guide-secondary-action"
          data-route={action.route}
        >
          <span class="min-w-0">{action.label}</span>
          <ChevronRight size={14} class="shrink-0 text-muted-foreground" aria-hidden="true" />
        </button>
      {/each}
    </div>
  {/if}
</section>
