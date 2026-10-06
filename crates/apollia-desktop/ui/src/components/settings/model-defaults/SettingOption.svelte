<script lang="ts">
  /**
   * SettingOption - one exclusive choice of a settings block: a radio, its
   * label, the sentence that explains it, and an optional figure with its
   * unit. The figure is committed when the field is left or Enter is pressed;
   * a refused figure shows its reason under the field, a written one shows
   * "saved" for a moment.
   */
  import { t } from "svelte-i18n";
  import { Check } from "lucide-svelte";
  import { RadioItem } from "$lib/components/ui/radio";
  import { Input } from "$lib/components/ui/input";

  interface Props {
    /** The value this choice stands for. */
    value: string;
    label: string;
    hint: string;
    checked: boolean;
    disabled?: boolean;
    onchoose: (value: string) => void;
    /** The figure of the choice; omit for a choice without one. */
    figure?: string;
    /** Shown in an empty field. */
    placeholder?: string;
    unit?: string;
    /** Accessible name of the field. */
    figureLabel?: string;
    oncommit?: (typed: string) => void;
    /** Why the figure typed last was refused, already translated. */
    error?: string | null;
    /** True for a moment after a write went through. */
    saved?: boolean;
    "data-testid": string;
  }

  let {
    value,
    label,
    hint,
    checked,
    disabled = false,
    onchoose,
    figure,
    placeholder,
    unit,
    figureLabel,
    oncommit,
    error = null,
    saved = false,
    "data-testid": testid,
  }: Props = $props();

  const hasFigure = $derived(figure !== undefined);
  const inputId = $derived(`${testid}-field`);
  const radioId = $derived(`${testid}-radio`);

  function commit(event: Event) {
    oncommit?.((event.currentTarget as HTMLInputElement).value);
  }
</script>

<div class="flex items-start gap-3 py-2" data-testid={`${testid}-row`} data-checked={checked}>
  <RadioItem
    {value}
    {checked}
    {disabled}
    onchange={onchoose}
    id={radioId}
    class="mt-0.5"
    data-testid={testid}
  />
  <div class="min-w-0 flex-1">
    <label for={radioId} class="block text-label-md text-foreground">{label}</label>
    <p class="mt-0.5 max-w-md text-caption text-muted-foreground">{hint}</p>
    {#if error}
      <p class="mt-1 text-caption text-destructive" role="alert" data-testid={`${testid}-error`}>
        {error}
      </p>
    {:else if saved}
      <p
        class="mt-1 inline-flex items-center gap-1 text-caption text-success"
        role="status"
        data-testid={`${testid}-saved`}
      >
        <Check class="h-3 w-3" aria-hidden="true" />
        {$t("settings.model_defaults.saved")}
      </p>
    {/if}
  </div>
  {#if hasFigure}
    <div class="flex flex-none items-center gap-2">
      <Input
        id={inputId}
        size="sm"
        inputmode="decimal"
        class={error ? "w-24 border-destructive text-right" : "w-24 text-right"}
        value={figure}
        {placeholder}
        disabled={disabled || !checked}
        aria-label={figureLabel ?? label}
        aria-invalid={error ? "true" : undefined}
        onchange={commit}
        data-testid={inputId}
      />
      {#if unit}
        <span class="text-caption text-muted-foreground">{unit}</span>
      {/if}
    </div>
  {/if}
</div>
