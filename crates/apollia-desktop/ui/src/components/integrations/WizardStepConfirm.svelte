<script lang="ts">
  import { t } from "svelte-i18n";
  import { cn } from "$lib/utils";
  import { RadioGroup, RadioItem } from "$lib/components/ui/radio";

  type ApprovalLevel = "auto" | "ask" | "readonly";

  interface ApprovalOption {
    value: ApprovalLevel;
    labelKey: string;
    descKey: string;
  }

  interface Props {
    approvalLevel: ApprovalLevel;
    onchange: (level: ApprovalLevel) => void;
  }

  let { approvalLevel, onchange }: Props = $props();

  const APPROVAL_OPTIONS: ApprovalOption[] = [
    {
      value: "auto",
      labelKey: "integrations.wizard.approval_auto_label",
      descKey: "integrations.wizard.approval_auto_desc",
    },
    {
      value: "ask",
      labelKey: "integrations.wizard.approval_ask_label",
      descKey: "integrations.wizard.approval_ask_desc",
    },
    {
      value: "readonly",
      labelKey: "integrations.wizard.approval_readonly_label",
      descKey: "integrations.wizard.approval_readonly_desc",
    },
  ];
</script>

<div class="space-y-4" data-testid="wizard-step-confirm">
  <div>
    <h3 class="text-sm font-medium text-foreground">
      {$t("integrations.wizard.approval_title")}
    </h3>
    <p class="mt-0.5 text-xs text-muted-foreground">
      {$t("integrations.wizard.approval_subtitle")}
    </p>
  </div>

  <RadioGroup class="gap-3">
    {#each APPROVAL_OPTIONS as option (option.value)}
      <div
        class={cn(
          "flex cursor-pointer items-start gap-3 rounded-md border px-4 py-3 transition-colors duration-150",
          approvalLevel === option.value
            ? "border-primary bg-primary/5"
            : "border-border",
        )}
      >
        <RadioItem
          value={option.value}
          checked={approvalLevel === option.value}
          onchange={() => onchange(option.value)}
          id={`approval-${option.value}`}
          data-testid={`approval-${option.value}`}
        />
        <label
          class="flex-1 cursor-pointer space-y-0.5"
          for={`approval-${option.value}`}
        >
          <p class="text-sm font-medium text-foreground">{$t(option.labelKey)}</p>
          <p class="text-xs text-muted-foreground">{$t(option.descKey)}</p>
        </label>
      </div>
    {/each}
  </RadioGroup>
</div>
