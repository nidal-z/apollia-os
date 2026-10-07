<script lang="ts">
  /**
   * PanelSettingField - one setting of the panel of a model: its name, what
   * it changes on hover, and its control. Left empty, the setting is
   * automatic and the control says so.
   */
  import { t } from "svelte-i18n";
  import { CircleHelp } from "lucide-svelte";
  import { Input } from "$lib/components/ui/input";
  import { Select } from "$lib/components/ui/select";
  import { Textarea } from "$lib/components/ui/textarea";
  import { Tooltip } from "$lib/components/ui/tooltip";
  import { isAuto, type DraftOption, type SettingField } from "./modelDetail";

  interface Props {
    field: SettingField;
    /** What is typed; empty for automatic. */
    value: string;
    /** The catalogue key of the refusal of what is typed, if any. */
    errorKey?: string;
    disabled?: boolean;
    /** The tab the setting belongs to, which names its `data-testid`. */
    scope: "load" | "inference";
    /** The draft models a `draft` setting is chosen among. */
    drafts?: readonly DraftOption[];
  }

  let {
    field,
    value = $bindable(""),
    errorKey,
    disabled = false,
    scope,
    drafts = [],
  }: Props = $props();

  const id = $derived(`ai-panel-${scope}-field-${field.key}`);
  const auto = $derived($t("ai.panel.auto"));
  const numeric = $derived(field.kind === "integer" || field.kind === "number");
  const range = $derived(
    field.min !== undefined && field.max !== undefined
      ? { min: field.min, max: field.max }
      : null,
  );
</script>

<div
  class="flex flex-col gap-1"
  data-testid="ai-panel-{scope}-row"
  data-key={field.key}
  data-auto={isAuto(value)}
>
  <div class="flex items-center gap-1.5">
    <label for={id} class="text-caption font-medium text-foreground">{$t(field.labelKey)}</label>
    <Tooltip content={$t(field.helpKey)}>
      <span
        class="inline-flex text-muted-foreground"
        role="img"
        aria-label={$t(field.helpKey)}
        data-testid="ai-panel-{scope}-help"
      >
        <CircleHelp size={12} strokeWidth={1.75} aria-hidden="true" />
      </span>
    </Tooltip>
  </div>
  {#if field.kind === "boolean"}
    <Select size="sm" {id} bind:value {disabled} data-testid={id}>
      <option value="">{auto}</option>
      <option value="true">{$t("ai.panel.on")}</option>
      <option value="false">{$t("ai.panel.off")}</option>
    </Select>
  {:else if field.kind === "draft"}
    <Select size="sm" {id} bind:value {disabled} data-testid={id}>
      <option value="">{$t("ai.panel.load.draft.none")}</option>
      {#each drafts as draft (draft.id)}
        <option value={draft.id} data-available={draft.available}>
          {draft.available
            ? draft.parameters
              ? `${draft.id} · ${draft.parameters}`
              : draft.id
            : $t("ai.panel.load.draft.unavailable", { values: { id: draft.id } })}
        </option>
      {/each}
    </Select>
  {:else if field.kind === "choice"}
    <Select size="sm" {id} bind:value {disabled} data-testid={id}>
      <option value="">{auto}</option>
      {#each field.choices ?? [] as choice (choice)}
        <option value={choice}>{choice}</option>
      {/each}
    </Select>
  {:else if field.kind === "multiline" || field.kind === "lines"}
    <Textarea
      size="sm"
      {id}
      bind:value
      {disabled}
      placeholder={auto}
      rows={field.kind === "lines" ? 2 : 4}
      class={field.kind === "multiline" ? "font-mono" : ""}
      data-testid={id}
    />
  {:else}
    <Input
      size="sm"
      {id}
      type="text"
      inputmode={numeric ? "decimal" : "text"}
      bind:value
      {disabled}
      placeholder={auto}
      aria-invalid={errorKey ? "true" : undefined}
      data-testid={id}
    />
  {/if}
  {#if errorKey}
    <p class="m-0 text-caption text-destructive" data-testid="ai-panel-{scope}-error">
      {$t(errorKey, { values: range ?? { min: field.min ?? 0, max: field.max ?? 0 } })}
    </p>
  {/if}
</div>
