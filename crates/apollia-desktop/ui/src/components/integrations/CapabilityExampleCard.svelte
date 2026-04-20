<script lang="ts">
  import { t } from "svelte-i18n";
  import { Sparkles, ArrowRight } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";

  interface Props {
    title: string;
    description: string;
    prompt: string;
    ontry: (prompt: string) => void;
  }

  let { title, description, prompt, ontry }: Props = $props();
</script>

<div
  class="flex items-start gap-3 rounded-md border border-border bg-card/60 p-3 transition-colors hover:border-primary/40"
  data-testid="capability-example-card"
>
  <Sparkles size={16} class="mt-0.5 shrink-0 text-primary" aria-hidden="true" />
  <div class="flex-1 space-y-1">
    <p class="text-sm font-medium text-foreground">{title}</p>
    <p class="text-xs text-muted-foreground">{description}</p>
    <div class="pt-1">
      <Button
        size="sm"
        variant="outline"
        onclick={() => ontry(prompt)}
        data-testid="capability-example-try"
      >
        {$t("integrations.wizard.coaching.try")}
        <ArrowRight size={12} class="ml-1" aria-hidden="true" />
      </Button>
    </div>
  </div>
</div>
