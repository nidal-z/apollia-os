<script lang="ts">
  /**
   * ModelInferenceTab - the Inference tab of the panel of a model: the
   * request settings of a preset in folded sections, structured output, and
   * the switches the card of the model declares, to read. No system prompt:
   * a preset never carries one.
   */
  import { t } from "svelte-i18n";
  import { Checkbox } from "$lib/components/ui/checkbox";
  import { Disclosure } from "$lib/components/ui/disclosure";
  import { Textarea } from "$lib/components/ui/textarea";
  import type { AuthorField } from "$lib/ipc/modelDetail";
  import PanelSettingField from "./PanelSettingField.svelte";
  import {
    INFERENCE_FIELDS,
    INFERENCE_SECTIONS,
    SCHEMA_ERROR_KEY,
    authorFieldRows,
    fieldsOf,
    type PanelForm,
  } from "./modelDetail";

  interface Props {
    form: PanelForm;
    errors: Record<string, string>;
    authorFields: AuthorField[];
  }

  let { form = $bindable(), errors, authorFields }: Props = $props();

  const author = $derived(authorFieldRows(authorFields));
</script>

<div class="flex flex-col gap-3" data-testid="ai-panel-inference">
  {#each INFERENCE_SECTIONS as section (section)}
    <Disclosure testid="ai-panel-inference-section-{section}">
      {#snippet summary()}
        <span class="font-medium text-foreground">{$t(`ai.panel.inference.section.${section}`)}</span>
      {/snippet}
      {#if section === "structured"}
        <div class="flex flex-col gap-2">
          <label class="flex items-center gap-2 text-caption font-medium text-foreground">
            <Checkbox
              checked={form.structured}
              onchange={(checked) => (form.structured = checked)}
              data-testid="ai-panel-structured-toggle"
            />
            {$t("ai.panel.inference.structured.label")}
          </label>
          {#if form.structured}
            <p class="m-0 text-caption text-muted-foreground" data-testid="ai-panel-structured-help">
              {$t("ai.panel.inference.structured.help")}
            </p>
            <Textarea
              size="sm"
              bind:value={form.schema}
              rows={6}
              class="font-mono"
              aria-label={$t("ai.panel.inference.structured.schema_label")}
              placeholder={$t("ai.panel.inference.structured.placeholder")}
              data-testid="ai-panel-structured-schema"
            />
            {#if errors[SCHEMA_ERROR_KEY]}
              <p class="m-0 text-caption text-destructive" data-testid="ai-panel-structured-error">
                {$t(errors[SCHEMA_ERROR_KEY])}
              </p>
            {/if}
          {/if}
        </div>
      {:else}
        <div class="flex flex-col gap-3">
          {#each fieldsOf(INFERENCE_FIELDS, section) as field (field.key)}
            <PanelSettingField
              {field}
              bind:value={form.request[field.key]}
              errorKey={errors[field.key]}
              scope="inference"
            />
          {/each}
        </div>
      {/if}
    </Disclosure>
  {/each}

  {#if author.length > 0}
    <section class="flex flex-col gap-2 border-t border-border pt-3" data-testid="ai-panel-author-fields">
      <p class="m-0 text-caption font-medium text-foreground">{$t("ai.panel.inference.author.title")}</p>
      <p class="m-0 text-caption text-muted-foreground">{$t("ai.panel.inference.author.hint")}</p>
      <ul class="m-0 flex list-none flex-col gap-2 p-0">
        {#each author as row (row.name)}
          <li class="flex flex-col gap-0.5 rounded-md border border-border p-2" data-testid="ai-panel-author-field">
            <div class="flex items-baseline justify-between gap-2">
              <code class="text-caption text-foreground" data-testid="ai-panel-author-name">{row.name}</code>
              <span class="text-caption text-muted-foreground" data-testid="ai-panel-author-default">
                {$t("ai.panel.inference.author.default", {
                  values: { value: row.defaultKey ? $t(row.defaultKey) : (row.defaultText ?? "") },
                })}
              </span>
            </div>
            {#if row.values.length > 0}
              <p class="m-0 text-caption text-muted-foreground" data-testid="ai-panel-author-values">
                {$t("ai.panel.inference.author.values", { values: { values: row.values.join(", ") } })}
              </p>
            {/if}
            <p class="m-0 text-caption text-foreground" data-testid="ai-panel-author-explanation">
              {row.explanation}
            </p>
          </li>
        {/each}
      </ul>
    </section>
  {/if}
</div>
