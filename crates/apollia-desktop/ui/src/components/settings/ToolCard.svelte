<script lang="ts">
  import { ChevronRight, AlertTriangle } from "lucide-svelte";
  import { Toggle } from "$lib/components/ui/toggle";
  import { Button } from "$lib/components/ui/button";
  import { EntityCard } from "$lib/components/operator";
  import type { ToolStatusDto } from "$lib/stores/toolGovernance";

  interface Props {
    tool: ToolStatusDto;
    title: string;
    description?: string;
    icon?: typeof ChevronRight;
    warning?: string | null;
    canConfigure: boolean;
    busy?: boolean;
    onToggle: (enabled: boolean) => Promise<void> | void;
    onConfigure: () => void;
  }

  let {
    tool,
    title,
    description,
    icon: IconComponent,
    warning,
    canConfigure,
    busy = false,
    onToggle,
    onConfigure,
  }: Props = $props();

  let toggling = $state(false);

  async function handleChange(checked: boolean): Promise<void> {
    if (toggling) return;
    toggling = true;
    try {
      await onToggle(checked);
    } finally {
      toggling = false;
    }
  }
</script>

<EntityCard
  {title}
  subtitle={tool.name}
  data-testid="tool-card-{tool.name}"
>
  {#snippet icon()}
    {#if IconComponent}
      <IconComponent size={16} aria-hidden="true" />
    {/if}
  {/snippet}
  {#snippet trailing()}
    <Toggle
      checked={tool.enabled}
      loading={toggling || busy}
      onchange={handleChange}
      aria-label={tool.enabled ? "Désactiver" : "Activer"}
      data-testid="tool-toggle-{tool.name}"
    />
  {/snippet}
  {#snippet body()}
    {#if description}
      <p class="text-xs text-muted-foreground">{description}</p>
    {/if}
    {#if warning}
      <p class="inline-flex items-center gap-1 text-xs text-amber-700 dark:text-amber-400">
        <AlertTriangle size={12} aria-hidden="true" />
        {warning}
      </p>
    {/if}
  {/snippet}
  {#snippet actions()}
    {#if canConfigure}
      <Button
        variant="ghost"
        size="sm"
        onclick={onConfigure}
        data-testid="tool-configure-{tool.name}"
      >
        Configurer
        <ChevronRight size={14} class="ml-1" aria-hidden="true" />
      </Button>
    {/if}
  {/snippet}
</EntityCard>
