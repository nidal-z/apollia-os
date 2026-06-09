<script lang="ts">
  // One editable step row: label field, remove button, inline error.
  import { t } from "svelte-i18n";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { Trash2 } from "lucide-svelte";
  import type { PlanStep } from "$lib/ipc/planMode";

  interface Props {
    step: PlanStep;
    errorMessage?: string | null;
    onLabelChange: (stepId: string, label: string) => void;
    onRemove: (stepId: string) => void;
  }

  let { step, errorMessage = null, onLabelChange, onRemove }: Props = $props();

  function handleInput(event: Event): void {
    const target = event.currentTarget as HTMLInputElement;
    onLabelChange(step.step_id, target.value);
  }
</script>

<li class="space-y-1" data-testid="plan-step-editor" data-step-id={step.step_id}>
  <div class="flex items-center gap-2">
    <span class="font-mono text-xs text-muted-foreground">{step.step_id}</span>
    <Input
      size="sm"
      value={step.description}
      oninput={handleInput}
      placeholder={$t("plan_mode.edit_step_label_placeholder")}
      class="flex-1"
      aria-invalid={errorMessage ? "true" : undefined}
    />
    <Button
      variant="ghost"
      size="icon-sm"
      onclick={() => onRemove(step.step_id)}
      aria-label={$t("plan_mode.remove_step")}
      data-testid="plan-step-remove"
    >
      <Trash2 size={14} class="text-destructive" />
    </Button>
  </div>
  {#if errorMessage}
    <p class="pl-7 text-xs text-destructive" data-testid="plan-step-error">
      {$t(errorMessage)}
    </p>
  {/if}
</li>
