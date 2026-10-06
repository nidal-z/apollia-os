<script lang="ts">
  /**
   * DiscoverCard - the card of the model chosen in the discovery window: its
   * name and figures, what it is and what it can do, then its variants for
   * this computer and the download of the chosen one.
   */
  import { locale, t } from "svelte-i18n";
  import { Check, Copy, Download, Heart, WifiOff, X } from "lucide-svelte";
  import { cn, formatRelativeTime } from "$lib/utils";
  import { Button } from "$lib/components/ui/button";
  import { Tooltip } from "$lib/components/ui/tooltip";
  import CapabilityIcon from "./CapabilityIcon.svelte";
  import ExplainedBadge from "./ExplainedBadge.svelte";
  import { capabilitiesOf, capabilityLabelKey } from "./explanations";
  import { gigabytes } from "./aiModels";
  import {
    countLabel,
    levelBits,
    parameterLabel,
    publisherKindKeys,
    variantKey,
    verdictKeys,
    verdictVariant,
    type CardDetail,
    type DiscoverEntry,
    type DownloadAction,
  } from "./discover";
  import type { ModelFormat, ModelVariant } from "$lib/ipc/modelHub";

  interface Props {
    entry: DiscoverEntry;
    /** What the repositories answered; null while they are asked or when none did. */
    detail: CardDetail | null;
    detailLoading: boolean;
    /** The format the filter keeps, shown among the facts. */
    format: ModelFormat;
    variants: readonly ModelVariant[];
    chosen: ModelVariant | null;
    action: DownloadAction;
    onpick: (variant: ModelVariant) => void;
    ondownload: () => void;
    oncopy: () => void;
    onclose: () => void;
  }

  let {
    entry,
    detail,
    detailLoading,
    format,
    variants,
    chosen,
    action,
    onpick,
    ondownload,
    oncopy,
    onclose,
  }: Props = $props();

  const lang = $derived($locale ?? "fr");
  const embeddings = $derived(entry.category === "embeddings");
  const capabilities = $derived(capabilitiesOf(detail?.capabilities ?? entry.capabilities));
  const params = $derived(parameterLabel(entry.parameters, lang));
  const architecture = $derived(entry.architecture ?? detail?.architecture ?? null);
  const updatedAt = $derived(detail?.updatedAt ?? entry.updatedAt);
  const kind = $derived(publisherKindKeys(entry.publisherKind));
  const memory = $derived(
    entry.minMemoryGb === null
      ? null
      : new Intl.NumberFormat(lang, { maximumFractionDigits: 1 }).format(entry.minMemoryGb),
  );

  const STATUS_KEYS: Record<DownloadAction, string> = {
    unavailable: "ai.discover.status.unavailable",
    downloaded: "ai.discover.status.downloaded",
    downloading: "ai.discover.status.downloading",
    offline: "ai.discover.status.offline",
    confirm: "ai.discover.status.too_large",
    download: "ai.discover.status.ready",
  };

  function memoryOf(variant: ModelVariant): string | null {
    if (variant.memory_gb === null || variant.memory_gb === undefined) return null;
    return new Intl.NumberFormat(lang, { minimumFractionDigits: 1, maximumFractionDigits: 1 }).format(
      variant.memory_gb,
    );
  }
</script>

<div class="flex min-h-0 min-w-0 flex-1 flex-col gap-4 overflow-auto p-5" data-testid="ai-discover-card">
  <header class="flex flex-col gap-2">
    <div class="flex items-center gap-2">
      <h2 class="min-w-0 truncate text-lg font-semibold text-foreground" data-testid="ai-discover-card-name">
        {entry.name}
      </h2>
      <Tooltip
        content={$t("ai.menu.copy_id")}
        onclick={oncopy}
        triggerLabel={$t("ai.menu.copy_id")}
        triggerTestid="ai-discover-card-copy"
        triggerClass="h-7 w-7 justify-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
      >
        <Copy size={14} aria-hidden="true" />
      </Tooltip>
      <Tooltip
        content={$t("common.close")}
        onclick={onclose}
        triggerLabel={$t("common.close")}
        triggerTestid="ai-discover-close"
        triggerClass="ml-auto h-7 w-7 justify-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
      >
        <X size={16} aria-hidden="true" />
      </Tooltip>
    </div>
    <div class="flex flex-wrap items-center gap-3 text-xs text-muted-foreground" data-testid="ai-discover-card-figures">
      {#if detail && detail.downloads !== null}
        <Tooltip content={$t("ai.discover.downloads_explanation")}>
          <span class="inline-flex items-center gap-1" data-testid="ai-discover-card-downloads">
            <Download size={12} aria-hidden="true" />
            {countLabel(detail.downloads, lang)}
          </span>
        </Tooltip>
      {/if}
      {#if detail && detail.likes !== null}
        <Tooltip content={$t("ai.discover.likes_explanation")}>
          <span class="inline-flex items-center gap-1" data-testid="ai-discover-card-likes">
            <Heart size={12} aria-hidden="true" />
            {countLabel(detail.likes, lang)}
          </span>
        </Tooltip>
      {/if}
      {#if updatedAt}
        <span data-testid="ai-discover-card-updated">
          {$t("ai.discover.updated", { values: { when: formatRelativeTime(updatedAt, lang) } })}
        </span>
      {/if}
      {#if entry.inSelection}
        <ExplainedBadge
          label={$t("ai.discover.selection")}
          explanation={$t("ai.discover.selection_explanation")}
          variant="primary"
          testid="ai-discover-card-selection"
        />
      {/if}
      {#if kind}
        <ExplainedBadge
          label={$t(kind.label)}
          explanation={$t(kind.explanation)}
          testid="ai-discover-card-publisher-kind"
        />
      {/if}
    </div>
  </header>

  <section class="flex flex-col gap-3 rounded-lg border border-border p-4" data-testid="ai-discover-card-facts">
    {#if entry.description}
      <p class="text-sm text-foreground" data-testid="ai-discover-card-description">{entry.description}</p>
    {/if}
    <dl class="flex flex-wrap gap-x-5 gap-y-2 text-sm">
      <div class="flex items-baseline gap-1.5">
        <dt class="text-overline text-muted-foreground">{$t("ai.discover.fact.publisher")}</dt>
        <dd class="text-foreground" data-testid="ai-discover-card-publisher">{entry.publisher}</dd>
      </div>
      {#if params}
        <div class="flex items-baseline gap-1.5">
          <dt class="text-overline text-muted-foreground">{$t("ai.discover.fact.parameters")}</dt>
          <dd class="text-foreground" data-testid="ai-discover-card-parameters">
            {$t(params.unitKey, { values: { n: params.value } })}
          </dd>
        </div>
      {/if}
      {#if architecture}
        <div class="flex items-baseline gap-1.5">
          <dt class="text-overline text-muted-foreground">{$t("ai.discover.fact.architecture")}</dt>
          <dd class="font-mono text-foreground" data-testid="ai-discover-card-architecture">{architecture}</dd>
        </div>
      {/if}
      <div class="flex items-baseline gap-1.5">
        <dt class="text-overline text-muted-foreground">{$t("ai.discover.fact.domain")}</dt>
        <dd class="text-foreground" data-testid="ai-discover-card-domain">
          {$t(embeddings ? "ai.discover.category_embeddings" : "ai.kind.language")}
        </dd>
      </div>
      <div class="flex items-center gap-1.5">
        <dt class="text-overline text-muted-foreground">{$t("ai.discover.fact.formats")}</dt>
        <dd>
          <ExplainedBadge
            label={(embeddings ? "gguf" : format).toUpperCase()}
            explanation={$t("ai.format_explanation")}
            help
            testid="ai-discover-card-format"
          />
        </dd>
      </div>
      {#if memory !== null}
        <div class="flex items-baseline gap-1.5">
          <dt class="text-overline text-muted-foreground">{$t("ai.discover.fact.min_memory")}</dt>
          <dd class="text-foreground" data-testid="ai-discover-card-memory">
            {$t("ai.installed.size", { values: { size: memory } })}
          </dd>
        </div>
      {/if}
      {#if entry.license}
        <div class="flex items-baseline gap-1.5">
          <dt class="text-overline text-muted-foreground">{$t("ai.discover.fact.license")}</dt>
          <dd class="text-foreground" data-testid="ai-discover-card-license">{entry.license}</dd>
        </div>
      {/if}
    </dl>
    {#if embeddings}
      <div class="flex flex-col gap-1" data-testid="ai-discover-card-purpose">
        <p class="text-overline text-muted-foreground">{$t("ai.discover.fact.purpose")}</p>
        <p class="text-sm text-foreground">{$t("ai.kind.embeddings_explanation")}</p>
        {#if entry.recommendedHere}
          <p class="text-sm text-foreground" data-testid="ai-discover-card-recommended-here">
            {$t("ai.discover.recommended_here_explanation")}
          </p>
        {/if}
      </div>
    {:else if capabilities.length > 0}
      <div class="flex flex-wrap items-center gap-3" data-testid="ai-discover-card-capabilities">
        <span class="text-overline text-muted-foreground">{$t("ai.discover.fact.capabilities")}</span>
        {#each capabilities as capability (capability)}
          <span class="inline-flex items-center gap-1.5 text-sm text-foreground">
            <CapabilityIcon {capability} />
            {$t(capabilityLabelKey(capability))}
          </span>
        {/each}
      </div>
    {/if}
  </section>

  <section class="flex flex-col gap-2" data-testid="ai-discover-card-download">
    <h3 class="text-sm font-semibold text-foreground">{$t("ai.discover.download_title")}</h3>
    <div class="flex flex-col gap-3 rounded-lg border border-border p-4">
      {#if detailLoading}
        <p class="text-sm text-muted-foreground" data-testid="ai-discover-variants-loading">
          {$t("ai.discover.variants_loading")}
        </p>
      {:else if variants.length > 0}
        <ul class="flex flex-col gap-1" role="radiogroup" aria-label={$t("ai.discover.download_title")}>
          {#each variants as variant (variantKey(variant))}
            {@const picked = chosen !== null && variantKey(chosen) === variantKey(variant)}
            {@const verdict = verdictKeys(variant.compatibility)}
            {@const footprint = memoryOf(variant)}
            <li
              class={cn(
                "flex flex-wrap items-center gap-2 rounded-md border px-3 py-2 text-sm",
                picked ? "border-primary/50 bg-muted" : "border-border",
              )}
              data-testid="ai-discover-variant"
              data-variant-level={variant.level}
            >
              <button
                type="button"
                role="radio"
                aria-checked={picked}
                class="font-medium text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
                onclick={() => onpick(variant)}
                data-testid="ai-discover-variant-pick"
              >
                {embeddings
                  ? entry.name
                  : $t("ai.discover.variant_bits", { values: { bits: levelBits(variant.level) } })}
              </button>
              <ExplainedBadge
                label={variant.format.toUpperCase()}
                explanation={$t("ai.format_explanation")}
                testid="ai-discover-variant-format"
              />
              {#if variant.recommended && !embeddings}
                <ExplainedBadge
                  label={$t("ai.discover.recommended")}
                  explanation={$t("ai.discover.recommended_explanation")}
                  variant="primary"
                  testid="ai-discover-variant-recommended"
                />
              {/if}
              {#if verdict && !embeddings}
                <ExplainedBadge
                  label={$t(verdict.label)}
                  explanation={$t(verdict.explanation)}
                  variant={verdictVariant(variant.compatibility)}
                  testid="ai-discover-variant-verdict"
                />
              {/if}
              {#if variant.downloaded}
                <ExplainedBadge
                  label={$t("ai.discover.on_this_computer")}
                  explanation={$t("ai.discover.on_this_computer_explanation")}
                  variant="success"
                  testid="ai-discover-variant-downloaded"
                />
              {/if}
              <span class="ml-auto whitespace-nowrap text-muted-foreground" data-testid="ai-discover-variant-size">
                {$t("ai.installed.size", { values: { size: gigabytes(variant.size_bytes, lang) } })}
                {#if footprint !== null}
                  · {$t("ai.discover.variant_memory", { values: { size: footprint } })}
                {/if}
              </span>
            </li>
          {/each}
        </ul>
      {/if}
      <div class="flex flex-wrap items-center gap-3">
        <p class="flex min-w-0 flex-1 items-start gap-2 text-sm text-muted-foreground" data-testid="ai-discover-status">
          {#if action === "offline"}
            <WifiOff size={14} class="mt-0.5 shrink-0 text-warning-a11y" aria-hidden="true" />
          {:else if action === "downloaded"}
            <Check size={14} class="mt-0.5 shrink-0 text-success-a11y" aria-hidden="true" />
          {/if}
          <span>{detailLoading ? "" : $t(STATUS_KEYS[action])}</span>
        </p>
        {#if chosen !== null || action === "offline"}
          <Button
            variant="primary-solid"
            size="sm"
            disabled={detailLoading || (action !== "download" && action !== "confirm")}
            onclick={ondownload}
            data-testid="ai-discover-download"
          >
            <Download size={14} aria-hidden="true" />
            {#if chosen !== null && (action === "download" || action === "confirm")}
              {$t(action === "confirm" ? "ai.discover.download_anyway" : "ai.discover.download", {
                values: { size: gigabytes(chosen.size_bytes, lang) },
              })}
            {:else}
              {$t("ai.discover.download_plain")}
            {/if}
          </Button>
        {/if}
      </div>
    </div>
  </section>
</div>
