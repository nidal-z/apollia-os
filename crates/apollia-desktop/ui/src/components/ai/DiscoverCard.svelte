<script lang="ts">
  /**
   * DiscoverCard - the card of the model chosen in the discovery window: its
   * name and figures, what it is and what it can do, its download options
   * for this computer with the download of the chosen one or a conversation
   * on it, then what its author defines and its README on demand.
   */
  import { locale, t } from "svelte-i18n";
  import { ArrowLeft, Check, Copy, Download, ExternalLink, Heart, MessageSquarePlus, WifiOff, X } from "lucide-svelte";
  import { formatRelativeTime } from "$lib/utils";
  import { Button } from "$lib/components/ui/button";
  import { Tooltip } from "$lib/components/ui/tooltip";
  import CapabilityIcon from "./CapabilityIcon.svelte";
  import DiscoverOptions from "./DiscoverOptions.svelte";
  import DiscoverReadme from "./DiscoverReadme.svelte";
  import DiscoverSettings from "./DiscoverSettings.svelte";
  import ExplainedBadge from "./ExplainedBadge.svelte";
  import { capabilitiesOf, capabilityLabelKey } from "./explanations";
  import { gigabytes } from "./aiModels";
  import {
    countLabel,
    parameterLabel,
    publisherKindKeys,
    type CardDetail,
    type DiscoverEntry,
    type DownloadAction,
  } from "./discover";
  import { domainKey, purposeKey, showsFormat, type ConversationAction } from "./discoverCard";
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
    /** What "Use in a new conversation" does for the chosen variant. */
    conversation: ConversationAction;
    /** A conversation is being opened. */
    opening: boolean;
    /** The page "See the source" opens; null when nothing names one. */
    sourceUrl: string | null;
    /** The repository the README is read from; null when nothing names one. */
    readmeRepo: string | null;
    onpick: (variant: ModelVariant) => void;
    /** Back to the variant recommended for this computer. */
    ondefault: () => void;
    onsource: () => void;
    onconversation: () => void;
    ondownload: () => void;
    oncopy: () => void;
    /** Back to the gallery the card was opened from. */
    onback: () => void;
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
    conversation,
    opening,
    sourceUrl,
    readmeRepo,
    onpick,
    ondefault,
    onsource,
    onconversation,
    ondownload,
    oncopy,
    onback,
    onclose,
  }: Props = $props();

  const lang = $derived($locale ?? "fr");
  const embeddings = $derived(entry.category === "embeddings");
  const purpose = $derived(purposeKey(entry.category));
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

  const footprint = $derived(
    chosen?.memory_gb === null || chosen?.memory_gb === undefined
      ? null
      : new Intl.NumberFormat(lang, { minimumFractionDigits: 1, maximumFractionDigits: 1 }).format(
          chosen.memory_gb,
        ),
  );
</script>

<div class="flex min-h-0 min-w-0 flex-1 flex-col gap-4 overflow-auto px-6 py-5" data-testid="ai-discover-card">
  <header class="flex flex-col gap-2">
    <button
      type="button"
      class="-ml-1.5 flex w-fit items-center gap-1 rounded-md px-1.5 py-1 text-xs text-muted-foreground hover:bg-muted hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
      onclick={onback}
      data-testid="ai-discover-back"
    >
      <ArrowLeft size={13} aria-hidden="true" />
      {$t("ai.discover.back_to_gallery")}
    </button>
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
      {#if sourceUrl}
        <Button variant="outline" size="sm" class="ml-auto gap-1.5" onclick={onsource} data-testid="ai-discover-card-source">
          <ExternalLink size={14} aria-hidden="true" />
          {$t("ai.menu.source")}
        </Button>
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
          {$t(domainKey(entry.category))}
        </dd>
      </div>
      {#if showsFormat(entry.category)}
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
      {/if}
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
    {#if purpose}
      <div class="flex flex-col gap-1" data-testid="ai-discover-card-purpose">
        <p class="text-overline text-muted-foreground">{$t("ai.discover.fact.purpose")}</p>
        <p class="text-sm text-foreground">{$t(purpose)}</p>
        {#if entry.recommendedHere && embeddings}
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
      {:else if chosen !== null}
        <DiscoverOptions
          name={entry.name}
          category={entry.category}
          {capabilities}
          {variants}
          {chosen}
          {onpick}
          {ondefault}
        />
        {#if footprint !== null}
          <p class="text-xs text-muted-foreground" data-testid="ai-discover-chosen-memory">
            {$t("ai.discover.variant_memory", { values: { size: footprint } })}
          </p>
        {/if}
      {/if}
      <p class="flex items-start gap-2 text-sm text-muted-foreground" data-testid="ai-discover-status">
        {#if action === "offline"}
          <WifiOff size={14} class="mt-0.5 shrink-0 text-warning-a11y" aria-hidden="true" />
        {:else if action === "downloaded"}
          <Check size={14} class="mt-0.5 shrink-0 text-success-a11y" aria-hidden="true" />
        {/if}
        <span>{detailLoading ? "" : $t(STATUS_KEYS[action])}</span>
      </p>
      <div class="flex flex-wrap items-center justify-end gap-2">
        {#if action !== "downloaded" && (chosen !== null || action === "offline")}
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
        {#if conversation.kind === "ready"}
          <Button
            variant="primary-solid"
            size="sm"
            class="gap-1.5"
            loading={opening}
            disabled={opening}
            onclick={onconversation}
            data-testid="ai-discover-conversation"
          >
            <MessageSquarePlus size={14} aria-hidden="true" />
            {$t("ai.discover.conversation.use")}
          </Button>
        {:else}
          <Tooltip content={$t(conversation.reasonKey)} class="max-w-xs">
            {#snippet trigger(props)}
              <span
                {...props}
                role="button"
                tabindex="0"
                aria-disabled="true"
                class="inline-flex h-9 cursor-not-allowed items-center gap-2 rounded-md border border-border px-3 text-sm text-muted-foreground opacity-60 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
                data-testid="ai-discover-conversation"
                data-blocked={conversation.reasonKey}
              >
                <MessageSquarePlus size={14} aria-hidden="true" />
                {$t("ai.discover.conversation.use")}
              </span>
            {/snippet}
          </Tooltip>
        {/if}
      </div>
    </div>
  </section>

  <DiscoverSettings authorFields={entry.authorFields} defaults={entry.defaults} />

  {#if readmeRepo}
    {#key readmeRepo}
      <DiscoverReadme repo={readmeRepo} />
    {/key}
  {/if}
</div>
