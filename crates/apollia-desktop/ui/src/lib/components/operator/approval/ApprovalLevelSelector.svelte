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
    level: ApprovalLevel;
    onchange: (level: ApprovalLevel) => void;
  }

  let { level, onchange }: Props = $props();

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
  ];

  // The "readonly" level was removed on 2026-08-20. It persisted the same byte as
  // "auto", so the most restrictive label produced the least protective setting:
  // an autonomous agent called every write, delete and network operation of an MCP
  // server the operator had explicitly set to read-only. There is no per-server
  // read-only notion anywhere, neither in McpServerConfig nor in the engine, so
  // the level promised something nothing could deliver. Restoring it means
  // building that notion first, not adding an option back here.
</script>

<RadioGroup class="gap-2" data-testid="approval-level-selector">
  {#each APPROVAL_OPTIONS as option (option.value)}
    <div
      class={cn(
        "flex cursor-pointer items-start gap-3 rounded-md border px-3 py-2.5 transition-colors duration-fast",
        level === option.value
          ? "border-primary bg-primary/5"
          : "border-border",
      )}
    >
      <RadioItem
        value={option.value}
        checked={level === option.value}
        onchange={() => onchange(option.value)}
        id={`manage-approval-${option.value}`}
        data-testid={`manage-approval-${option.value}`}
      />
      <label
        class="flex-1 cursor-pointer space-y-0.5"
        for={`manage-approval-${option.value}`}
      >
        <p class="text-sm font-medium text-foreground">{$t(option.labelKey)}</p>
        <p class="text-xs text-muted-foreground">{$t(option.descKey)}</p>
      </label>
    </div>
  {/each}
</RadioGroup>
