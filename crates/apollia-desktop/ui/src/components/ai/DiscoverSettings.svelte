<script lang="ts">
  /**
   * DiscoverSettings - what the author of the model defines, read on its
   * card: the settings particular to the model, each with its default and
   * what it does, then the request settings the author advises. The card
   * states them; nothing here changes how a conversation runs.
   */
  import { locale, t } from "svelte-i18n";
  import type { AuthorField, CardDefaults } from "./discover";
  import { authorFieldRows, defaultParameterRows } from "./discoverCard";

  interface Props {
    authorFields: readonly AuthorField[];
    defaults: CardDefaults | null;
  }

  let { authorFields, defaults }: Props = $props();

  const lang = $derived($locale ?? "fr");
  const fields = $derived(authorFieldRows(authorFields));
  const parameters = $derived(defaultParameterRows(defaults, lang));
</script>

{#if fields.length > 0}
  <section class="flex flex-col gap-2" data-testid="ai-discover-author-fields">
    <div class="flex flex-wrap items-baseline gap-x-2">
      <h3 class="text-sm font-semibold text-foreground">{$t("ai.discover.author_fields.title")}</h3>
      <p class="text-xs text-muted-foreground">{$t("ai.discover.author_fields.subtitle")}</p>
    </div>
    <ul class="flex flex-col gap-3 rounded-lg border border-border p-4">
      {#each fields as field (field.name)}
        <li class="flex flex-col gap-0.5" data-testid="ai-discover-author-field">
          <p class="flex flex-wrap items-baseline gap-x-2 text-sm">
            <span class="font-medium text-foreground">{field.name}</span>
            <span class="font-mono text-xs text-muted-foreground">
              {$t(field.typeKey)}{#if field.values.length > 0}{" "}({field.values.join(", ")}){/if}
            </span>
            <span class="font-mono text-xs text-muted-foreground" data-testid="ai-discover-author-field-default">
              {$t("ai.discover.author_fields.default", {
                values: {
                  value: "key" in field.defaultValue ? $t(field.defaultValue.key) : field.defaultValue.text,
                },
              })}
            </span>
          </p>
          <p class="text-xs text-muted-foreground">{field.explanation}</p>
        </li>
      {/each}
    </ul>
  </section>
{/if}

{#if parameters.length > 0}
  <section class="flex flex-col gap-2" data-testid="ai-discover-defaults">
    <div class="flex flex-wrap items-baseline gap-x-2">
      <h3 class="text-sm font-semibold text-foreground">{$t("ai.discover.defaults.title")}</h3>
      <p class="text-xs text-muted-foreground">{$t("ai.discover.defaults.subtitle")}</p>
    </div>
    <dl class="flex flex-col gap-2 rounded-lg border border-border p-4 text-sm">
      {#each parameters as parameter (parameter.id)}
        <div class="flex items-baseline gap-3" data-testid="ai-discover-default-{parameter.id}">
          <dt class="text-foreground">{$t(parameter.labelKey)}</dt>
          <dd class="ml-auto rounded-full border border-border px-2 font-mono text-xs text-muted-foreground">
            {parameter.value ?? $t("ai.discover.defaults.off")}
          </dd>
        </div>
      {/each}
    </dl>
  </section>
{/if}
