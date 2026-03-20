<script lang="ts">
  import { t } from "svelte-i18n";
  import type { PipelineDefinitionView } from "$lib/types";
  import { Card, CardContent } from "$lib/components/ui/card";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";

  interface Props {
    definition: PipelineDefinitionView;
    onedit: (id: string) => void;
    ondelete: (id: string) => void;
  }

  let { definition, onedit, ondelete }: Props = $props();
</script>

<Card class="relative overflow-hidden" data-testid="pipeline-def-card-{definition.id}">
  <CardContent class="py-3">
    <div class="flex items-center gap-4">
      <div class="min-w-0 flex-1">
        <div class="flex items-center gap-2">
          <span class="text-sm font-medium" data-testid="pipeline-def-id-{definition.id}">
            {definition.id}
          </span>
          <Badge variant="outline" class={definition.enabled ? 'border-[var(--apollia-success)] text-[var(--apollia-success)]' : 'border-muted-foreground text-muted-foreground'}>
            {definition.enabled ? $t("pipelines.def_enabled") : $t("pipelines.def_disabled")}
          </Badge>
        </div>
        {#if definition.description}
          <p class="mt-0.5 text-xs text-muted-foreground" data-testid="pipeline-def-desc-{definition.id}">
            {definition.description}
          </p>
        {/if}
      </div>

      <div class="flex items-center gap-4 text-xs text-muted-foreground">
        <div class="text-center">
          <div class="font-medium text-foreground">{definition.steps.length}</div>
          <div>{$t("pipelines.steps")}</div>
        </div>
        <div class="text-center">
          <div class="font-medium text-foreground">{definition.on_failure}</div>
          <div>{$t("pipelines.def_on_failure")}</div>
        </div>
      </div>

      <div class="flex items-center gap-1">
        <Button
          size="sm"
          variant="ghost"
          onclick={() => onedit(definition.id)}
          data-testid="pipeline-def-edit-{definition.id}"
        >
          {$t("pipelines.def_edit")}
        </Button>
        <Button
          size="sm"
          variant="ghost"
          class="text-[hsl(var(--destructive))]"
          onclick={() => ondelete(definition.id)}
          data-testid="pipeline-def-delete-{definition.id}"
        >
          {$t("pipelines.def_delete")}
        </Button>
      </div>
    </div>
  </CardContent>
</Card>
