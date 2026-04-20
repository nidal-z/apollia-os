<script lang="ts">
  import { Toggle } from "$lib/components/ui/toggle";

  interface Props {
    label: string;
    description?: string;
    checked: boolean;
    disabled?: boolean;
    loading?: boolean;
    onChange?: (checked: boolean) => void;
    ariaLabel?: string;
    id?: string;
    "data-testid"?: string;
  }

  let {
    label,
    description,
    checked = $bindable(false),
    disabled = false,
    loading = false,
    onChange,
    ariaLabel,
    id,
    "data-testid": dataTestId,
  }: Props = $props();

  function handleChange(next: boolean) {
    checked = next;
    onChange?.(next);
  }
</script>

<div class="flex items-start justify-between gap-4 py-1.5" data-testid={dataTestId}>
  <div class="flex-1 min-w-0">
    <label
      for={id}
      class="text-sm font-medium text-foreground block cursor-pointer"
    >
      {label}
    </label>
    {#if description}
      <p class="mt-0.5 text-xs text-muted-foreground leading-relaxed">{description}</p>
    {/if}
  </div>
  <Toggle
    {id}
    {checked}
    {disabled}
    {loading}
    onchange={handleChange}
    aria-label={ariaLabel ?? label}
    data-testid={dataTestId ? `${dataTestId}-switch` : undefined}
  />
</div>
