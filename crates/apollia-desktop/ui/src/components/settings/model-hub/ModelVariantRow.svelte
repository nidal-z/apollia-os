<script lang="ts">
  /**
   * ModelVariantRow - one quantization group inside an expanded model card.
   *
   * Shows the quant label, the (possibly split) total size, the fit verdict
   * against the hardware budget, and a download action. A `too_large` verdict
   * keeps the action enabled but tinted; a live download disables it.
   */
  import { t } from "svelte-i18n";
  import { Download, Package } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import type { GgufGroup } from "./helpers";
  import { compatClass, compatIcon } from "./helpers";

  interface Props {
    group: GgufGroup;
    downloading: boolean;
    onDownload: (group: GgufGroup) => void;
  }

  let { group, downloading, onDownload }: Props = $props();

  const CompatIcon = $derived(group.compatibility ? compatIcon(group.compatibility) : null);
  const compatTooltip = $derived(
    group.compatibility === "fits"
      ? $t("settings.model_hub.detail.compat_fits")
      : group.compatibility === "might_fit"
        ? $t("settings.model_hub.detail.compat_might_fit")
        : group.compatibility === "too_large"
          ? $t("settings.model_hub.detail.compat_too_large")
          : "",
  );
</script>

<div
  class="flex items-center gap-3 rounded-md border border-border/60 bg-card px-3 py-2.5 transition-colors hover:border-border"
  data-testid="model-variant-{group.quantName}"
>
  <div class="min-w-0 flex-1">
    <div class="flex items-center gap-1.5">
      <span class="truncate font-mono text-body-xs font-medium text-foreground" title={group.fullName}>{group.quantName}</span>
      {#if group.isSplit}
        <span class="inline-flex shrink-0 items-center gap-0.5 rounded bg-muted px-1.5 py-0.5 text-overline text-muted-foreground">
          <Package size={11} aria-hidden="true" />
          {$t("settings.model_hub.detail.parts", { values: { count: group.files.length } })}
        </span>
      {/if}
    </div>
    <div class="mt-0.5 font-mono text-caption tabular-nums text-muted-foreground">{group.totalSizeHuman}</div>
  </div>

  {#if group.compatibility && CompatIcon}
    {@const Icon = CompatIcon}
    <span class="inline-flex items-center gap-1 text-caption font-medium {compatClass(group.compatibility)}" title={compatTooltip}>
      <Icon size={14} aria-hidden="true" />
    </span>
  {/if}

  <Button
    variant="default"
    size="sm"
    class="h-7 px-2.5 text-caption"
    loading={downloading}
    disabled={downloading}
    onclick={() => onDownload(group)}
    data-testid="model-variant-download-{group.quantName}"
  >
    {#snippet icon()}<Download size={13} />{/snippet}
    {group.isSplit
      ? $t("settings.model_hub.detail.files_count", { values: { count: group.files.length } })
      : $t("settings.model_hub.detail.download")}
  </Button>
</div>
