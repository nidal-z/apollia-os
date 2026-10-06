<script lang="ts">
  /**
   * DiscoverOptions - the download options of the card: the chosen variant,
   * and the list it opens, one row per variant, where another is picked or
   * the choice goes back to the one recommended for this computer.
   */
  import { locale, t } from "svelte-i18n";
  import { Check, ChevronsUpDown, ThumbsUp, Zap } from "lucide-svelte";
  import { Popover } from "$lib/components/ui/popover";
  import type { ModelVariant } from "$lib/ipc/modelHub";
  import { cn } from "$lib/utils";
  import CapabilityIcon from "./CapabilityIcon.svelte";
  import ExplainedBadge from "./ExplainedBadge.svelte";
  import ExplainedMark from "./ExplainedMark.svelte";
  import type { Capability } from "./explanations";
  import { gigabytes } from "./aiModels";
  import { variantKey, verdictKeys, verdictVariant, type DiscoverCategory } from "./discover";
  import { precisionLabel, showsFormat } from "./discoverCard";

  interface Props {
    /** The name of the model, which every row repeats. */
    name: string;
    category: DiscoverCategory;
    /** What the model can do, drawn on each row of a language model. */
    capabilities: readonly Capability[];
    variants: readonly ModelVariant[];
    chosen: ModelVariant;
    onpick: (variant: ModelVariant) => void;
    /** Back to the variant recommended for this computer. */
    ondefault: () => void;
  }

  let { name, category, capabilities, variants, chosen, onpick, ondefault }: Props = $props();

  let open = $state(false);

  const lang = $derived($locale ?? "fr");
  const format = $derived(showsFormat(category));

  function precision(variant: ModelVariant): string {
    const label = precisionLabel(variant, category);
    return label.kind === "bits"
      ? $t("ai.discover.variant_bits", { values: { bits: label.bits } })
      : label.text;
  }

  function size(variant: ModelVariant): string {
    return $t("ai.installed.size", { values: { size: gigabytes(variant.size_bytes, lang) } });
  }

  function pick(variant: ModelVariant): void {
    open = false;
    onpick(variant);
  }

  function useDefault(): void {
    open = false;
    ondefault();
  }
</script>

<Popover bind:open align="end" class="w-[min(36rem,85vw)] p-0">
  {#snippet trigger(props)}
    <button
      type="button"
      {...props}
      class="flex w-full items-center gap-2 rounded-md border border-border bg-muted/40 px-3 py-2 text-left text-sm text-foreground transition-colors hover:bg-muted focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
      aria-label={$t("ai.discover.options.title")}
      data-testid="ai-discover-options-trigger"
    >
      {#if format}
        <span class="rounded border border-border px-1.5 text-xs text-muted-foreground">
          {chosen.format.toUpperCase()}
        </span>
      {/if}
      <span class="min-w-0 truncate font-medium" data-testid="ai-discover-chosen-name">{name}</span>
      <span class="shrink-0 whitespace-nowrap rounded-full border border-border px-2 text-xs text-muted-foreground" data-testid="ai-discover-chosen-precision">
        {precision(chosen)}
      </span>
      {#if chosen.recommended}
        <ThumbsUp size={13} class="shrink-0 text-primary" aria-hidden="true" />
      {/if}
      <span class="ml-auto whitespace-nowrap text-muted-foreground" data-testid="ai-discover-chosen-size">
        {size(chosen)}
      </span>
      <ChevronsUpDown size={13} class="shrink-0 text-muted-foreground" aria-hidden="true" />
    </button>
  {/snippet}
  {#snippet content()}
    <div data-testid="ai-discover-options">
      <div class="flex items-center gap-3 border-b border-border px-3 py-2">
        <p class="text-sm font-medium text-foreground">{$t("ai.discover.options.title")}</p>
        <button
          type="button"
          class="ml-auto rounded-md px-2 py-1 text-sm text-muted-foreground transition-colors hover:bg-muted hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
          onclick={useDefault}
          data-testid="ai-discover-options-default"
        >
          {$t("ai.discover.options.use_default")}
        </button>
      </div>
      <ul class="flex max-h-80 flex-col gap-0.5 overflow-auto p-1.5" role="radiogroup" aria-label={$t("ai.discover.options.title")}>
        {#each variants as variant (variantKey(variant))}
          {@const picked = variantKey(chosen) === variantKey(variant)}
          {@const verdict = verdictKeys(variant.compatibility)}
          <li
            class={cn("flex items-center gap-2 rounded-md px-2 py-1.5 text-sm", picked && "bg-muted")}
            data-testid="ai-discover-variant"
            data-variant-level={variant.level}
          >
            <span class="inline-flex w-4 shrink-0 justify-center">
              {#if picked}
                <Check size={14} class="text-foreground" aria-hidden="true" />
              {/if}
            </span>
            {#if format}
              <ExplainedBadge
                label={variant.format.toUpperCase()}
                explanation={$t("ai.format_explanation")}
                testid="ai-discover-variant-format"
              />
            {/if}
            <button
              type="button"
              role="radio"
              aria-checked={picked}
              class="inline-flex min-w-0 items-center gap-2 rounded-sm text-left text-foreground hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
              onclick={() => pick(variant)}
              data-testid="ai-discover-variant-pick"
            >
              <span class="min-w-0 truncate font-medium">{name}</span>
              <span class="shrink-0 whitespace-nowrap rounded-full border border-border px-2 text-xs text-muted-foreground">
                {precision(variant)}
              </span>
            </button>
            {#if category === "language"}
              {#each capabilities as capability (capability)}
                <CapabilityIcon {capability} />
              {/each}
              {#if variant.fully_offloaded}
                <ExplainedMark
                  label={$t("ai.discover.offload.label")}
                  explanation={$t("ai.discover.offload.explanation")}
                  tone="success"
                  testid="ai-discover-variant-offload"
                >
                  <Zap size={12} strokeWidth={1.75} aria-hidden="true" />
                </ExplainedMark>
              {/if}
            {/if}
            <span class="ml-auto inline-flex shrink-0 items-center gap-2">
              {#if variant.recommended}
                <ExplainedMark
                  label={$t("ai.discover.recommended")}
                  explanation={$t("ai.discover.recommended_explanation")}
                  testid="ai-discover-variant-recommended"
                >
                  <ThumbsUp size={12} strokeWidth={1.75} aria-hidden="true" />
                </ExplainedMark>
              {/if}
              {#if variant.downloaded}
                <ExplainedBadge
                  label={$t("ai.discover.downloaded")}
                  explanation={$t("ai.discover.on_this_computer_explanation")}
                  variant="success"
                  testid="ai-discover-variant-downloaded"
                />
              {/if}
              {#if verdict && category !== "embeddings"}
                <ExplainedBadge
                  label={$t(verdict.label)}
                  explanation={$t(verdict.explanation)}
                  variant={verdictVariant(variant.compatibility)}
                  testid="ai-discover-variant-verdict"
                />
              {/if}
              <span class="whitespace-nowrap text-muted-foreground" data-testid="ai-discover-variant-size">
                {size(variant)}
              </span>
            </span>
          </li>
        {/each}
      </ul>
    </div>
  {/snippet}
</Popover>
