<script lang="ts">
  import { ChevronRight, AlertTriangle } from "lucide-svelte";
  import { Toggle } from "$lib/components/ui/toggle";
  import { Button } from "$lib/components/ui/button";
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

<article
  class="glass-card glass-border rounded-lg p-4"
  data-testid="tool-card-{tool.name}"
>
  <div class="flex items-start gap-3">
    <div class="mt-0.5 flex h-8 w-8 shrink-0 items-center justify-center rounded-md bg-muted/50 text-muted-foreground">
      {#if IconComponent}
        <IconComponent size={16} aria-hidden="true" />
      {/if}
    </div>
    <div class="flex-1 min-w-0">
      <div class="flex items-center gap-2">
        <h3 class="text-sm font-medium text-foreground">{title}</h3>
        <code class="text-[11px] text-muted-foreground">{tool.name}</code>
      </div>
      {#if description}
        <p class="mt-0.5 text-xs text-muted-foreground">{description}</p>
      {/if}
      {#if warning}
        <p class="mt-1 inline-flex items-center gap-1 text-xs text-amber-700 dark:text-amber-400">
          <AlertTriangle size={12} aria-hidden="true" />
          {warning}
        </p>
      {/if}
    </div>
    <div class="flex items-center gap-2">
      <Toggle
        checked={tool.enabled}
        loading={toggling || busy}
        onchange={handleChange}
        aria-label={tool.enabled ? "Désactiver" : "Activer"}
        data-testid="tool-toggle-{tool.name}"
      />
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
    </div>
  </div>
</article>
