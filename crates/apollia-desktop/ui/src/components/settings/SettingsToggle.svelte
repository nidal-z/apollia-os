<script module lang="ts">
  /**
   * Where the value of a named setting comes from, as the Rust side resolves
   * it (`apollia_core::config::SettingSource`).
   */
  export type SettingSource = "default" | "user" | "policy";

  export type SettingsToggleState = {
    /** True when the switch refuses a change. */
    disabled: boolean;
    /** The i18n key of the policy mention, null when the row is free. */
    policyLabelKey: string | null;
  };

  /**
   * What the row allows for a setting from `source`: a value the
   * organization policy imposes disables the switch and names the policy,
   * whatever the caller asked; any other source leaves `disabled` as given.
   */
  export function settingsToggleState(
    source: SettingSource | undefined,
    disabled: boolean,
  ): SettingsToggleState {
    const locked = source === "policy";
    return {
      disabled: disabled || locked,
      policyLabelKey: locked ? "settings.policy_locked" : null,
    };
  }
</script>

<script lang="ts">
  import { t } from "svelte-i18n";
  import { Lock } from "lucide-svelte";
  import { Toggle } from "$lib/components/ui/toggle";

  interface Props {
    label: string;
    description?: string;
    checked: boolean;
    disabled?: boolean;
    loading?: boolean;
    /** Source of the value; `"policy"` locks the row. */
    source?: SettingSource;
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
    source,
    onChange,
    ariaLabel,
    id,
    "data-testid": dataTestId,
  }: Props = $props();

  const rowState = $derived(settingsToggleState(source, disabled));

  function handleChange(next: boolean) {
    if (rowState.disabled) return;
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
    {#if rowState.policyLabelKey}
      <p
        class="mt-0.5 inline-flex items-center gap-1.5 text-xs text-muted-foreground"
        title={$t("settings.policy_locked_hint")}
        data-testid={dataTestId ? `${dataTestId}-policy` : undefined}
      >
        <Lock class="h-3 w-3" aria-hidden="true" />
        {$t(rowState.policyLabelKey)}
      </p>
    {/if}
  </div>
  <Toggle
    {id}
    {checked}
    disabled={rowState.disabled}
    {loading}
    onchange={handleChange}
    aria-label={ariaLabel ?? label}
    data-testid={dataTestId ? `${dataTestId}-switch` : undefined}
  />
</div>
