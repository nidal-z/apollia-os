<script lang="ts">
  /**
   * ModelInfoTab - the Infos tab of the panel of a model: what the model is,
   * in a fixed order, then the identifier an agent or a request names it by.
   */
  import { locale, t } from "svelte-i18n";
  import { Copy } from "lucide-svelte";
  import type { InstalledModel } from "$lib/ipc/modelHub";
  import type { ModelDetail } from "$lib/ipc/modelDetail";
  import { Tooltip } from "$lib/components/ui/tooltip";
  import CapabilityIcon from "./CapabilityIcon.svelte";
  import { capabilitiesOf } from "./explanations";
  import { gigabytes } from "./aiModels";
  import { infoRows } from "./modelDetail";

  interface Props {
    model: InstalledModel;
    detail: ModelDetail | null;
    oncopy: (text: string) => void;
  }

  let { model, detail, oncopy }: Props = $props();

  const lang = $derived($locale ?? "fr");
  const rows = $derived(infoRows(model, detail));
  const capabilities = $derived(capabilitiesOf(model.capabilities));
</script>

<div class="flex flex-col gap-4" data-testid="ai-panel-info">
  <dl class="m-0 flex flex-col gap-2">
    {#each rows as row (row.id)}
      <div class="flex items-start justify-between gap-3" data-testid="ai-panel-info-row" data-row={row.id}>
        <dt class="shrink-0 text-caption text-muted-foreground">{$t(row.labelKey)}</dt>
        <dd class="m-0 min-w-0 break-words text-right text-caption text-foreground" data-testid="ai-panel-info-{row.id}">
          {#if row.id === "capabilities"}
            {#if capabilities.length > 0}
              <span class="inline-flex gap-1">
                {#each capabilities as capability (capability)}
                  <CapabilityIcon {capability} />
                {/each}
              </span>
            {:else}
              <span class="text-muted-foreground">{$t("ai.panel.info.no_capability")}</span>
            {/if}
          {:else if row.id === "size"}
            {$t("ai.installed.size", { values: { size: gigabytes(model.size_bytes, lang) } })}
          {:else if row.value !== null}
            {row.value}
          {:else if row.valueKey}
            <span class={row.id === "domain" ? "" : "text-muted-foreground"}>{$t(row.valueKey)}</span>
          {/if}
        </dd>
      </div>
    {/each}
  </dl>

  <div class="flex flex-col gap-1.5 border-t border-border pt-3">
    <p class="m-0 text-caption font-medium text-foreground">{$t("ai.panel.info.usage_title")}</p>
    <p class="m-0 text-caption text-muted-foreground">{$t("ai.panel.info.usage_hint")}</p>
    <div class="flex items-center gap-1.5">
      <code
        class="min-w-0 flex-1 break-all rounded border border-border px-2 py-1 text-caption text-foreground"
        data-testid="ai-panel-identifier"
      >{model.id}</code>
      <Tooltip
        content={$t("ai.menu.copy_id")}
        onclick={() => oncopy(model.id)}
        triggerLabel={$t("ai.menu.copy_id")}
        triggerTestid="ai-panel-copy-id"
        triggerClass="h-7 w-7 shrink-0 justify-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
      >
        <Copy size={14} aria-hidden="true" />
      </Tooltip>
    </div>
  </div>
</div>
