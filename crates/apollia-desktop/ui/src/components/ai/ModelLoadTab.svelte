<script lang="ts">
  /**
   * ModelLoadTab - the Loading tab of the panel of a model: the load settings
   * of a GGUF model in two folded sections, each automatic until it is set;
   * an MLX model takes none, and the tab says so.
   */
  import { t } from "svelte-i18n";
  import { TriangleAlert } from "lucide-svelte";
  import { Disclosure } from "$lib/components/ui/disclosure";
  import PanelSettingField from "./PanelSettingField.svelte";
  import {
    LOAD_FIELDS,
    LOAD_SECTIONS,
    fieldsOf,
    loadTabView,
    type PanelForm,
  } from "./modelDetail";

  interface Props {
    format: "gguf" | "mlx";
    form: PanelForm;
    errors: Record<string, string>;
    /** Whether a preset keeps load settings; off, the fields are read only. */
    writable: boolean;
    /** Whether the model in memory will be relaunched by these settings. */
    relaunch: boolean;
  }

  let { format, form = $bindable(), errors, writable, relaunch }: Props = $props();
</script>

<div class="flex flex-col gap-3" data-testid="ai-panel-load" data-view={loadTabView(format)}>
  {#if loadTabView(format) === "none"}
    <p class="m-0 text-body-sm text-muted-foreground" data-testid="ai-panel-load-none">
      {$t("ai.panel.load.none")}
    </p>
  {:else}
    {#if !writable}
      <p class="m-0 text-caption text-muted-foreground" data-testid="ai-panel-load-readonly">
        {$t("ai.panel.load.readonly")}
      </p>
    {/if}
    {#if relaunch}
      <p
        class="m-0 flex items-start gap-2 rounded-md border border-warning/40 bg-warning/10 p-2 text-caption text-foreground"
        role="status"
        data-testid="ai-panel-relaunch-warning"
      >
        <TriangleAlert size={14} class="mt-0.5 shrink-0 text-warning-a11y" aria-hidden="true" />
        <span>{$t("ai.panel.load.relaunch_warning")}</span>
      </p>
    {/if}
    {#each LOAD_SECTIONS as section (section)}
      <Disclosure testid="ai-panel-load-section-{section}">
        {#snippet summary()}
          <span class="font-medium text-foreground">{$t(`ai.panel.load.section.${section}`)}</span>
        {/snippet}
        <div class="flex flex-col gap-3">
          {#each fieldsOf(LOAD_FIELDS, section) as field (field.key)}
            <PanelSettingField
              {field}
              bind:value={form.load[field.key]}
              errorKey={errors[field.key]}
              disabled={!writable}
              scope="load"
            />
          {/each}
        </div>
      </Disclosure>
    {/each}
  {/if}
</div>
