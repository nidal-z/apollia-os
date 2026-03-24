<script lang="ts">
  import { t } from "svelte-i18n";
  import type { PipelineDefinitionView } from "$lib/types";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";

  interface Props {
    definition: PipelineDefinitionView;
    onedit: (id: string) => void;
    ondelete: (id: string) => void;
  }

  let { definition, onedit, ondelete }: Props = $props();

  let agentNames = $derived(
    definition.steps.map((s) => s.agent).join(", "),
  );
</script>

<div class="glass-card-hover relative overflow-hidden" data-testid="pipeline-def-card">
  <!-- Status accent bar -->
  <div
    class="h-0.5 w-full {definition.enabled ? 'bg-primary' : 'bg-muted-foreground/20'}"
    data-testid="pipeline-def-status-bar"
  ></div>

  <div class="px-3.5 pt-3 pb-2.5">
    <!-- Header -->
    <div class="flex items-center justify-between">
      <h3 class="text-[13px] font-medium">{definition.id}</h3>
      <Badge variant={definition.enabled ? "default" : "secondary"}>
        {definition.enabled ? $t("pipelines.def_enabled") : $t("pipelines.def_disabled")}
      </Badge>
    </div>

    <!-- Info -->
    <p class="text-[11px] text-muted-foreground mt-1">
      {definition.steps.length} {$t("pipelines.steps")} — {agentNames}
    </p>

    {#if definition.description}
      <p class="text-[11px] text-muted-foreground mt-0.5 line-clamp-2">
        {definition.description}
      </p>
    {/if}

    <!-- Actions -->
    <div class="flex gap-2 mt-3">
      <Button size="sm" variant="outline" onclick={() => onedit(definition.id)} data-testid="pipeline-def-edit-btn">
        {$t("pipelines.def_edit")}
      </Button>
      <Button
        size="sm"
        variant="ghost"
        class="text-destructive"
        onclick={() => ondelete(definition.id)}
        data-testid="pipeline-def-delete-btn"
      >
        {$t("pipelines.def_delete")}
      </Button>
    </div>
  </div>
</div>
