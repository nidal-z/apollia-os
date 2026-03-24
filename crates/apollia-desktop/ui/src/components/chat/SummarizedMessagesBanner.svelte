<script lang="ts">
  import { slide } from "svelte/transition";
  import { t } from "svelte-i18n";
  import { ChevronDown, ChevronUp } from "lucide-svelte";

  interface Props {
    summarizedCount: number;
    summaryText: string;
  }

  let { summarizedCount, summaryText }: Props = $props();

  let isExpanded = $state(false);
</script>

<div
  class="rounded-lg border border-[#7c5fd6]/20 bg-white/[0.05] px-4 py-2"
  data-testid="summarized-banner"
>
  <button
    class="flex w-full items-center justify-between text-left"
    onclick={() => (isExpanded = !isExpanded)}
  >
    <span class="text-[11px] text-muted-foreground">
      {$t("chat.summarized_messages", { values: { count: summarizedCount } })}
    </span>
    {#if isExpanded}
      <ChevronUp size={12} class="text-muted-foreground/50" />
    {:else}
      <ChevronDown size={12} class="text-muted-foreground/50" />
    {/if}
  </button>

  {#if isExpanded}
    <div
      class="mt-2 rounded bg-white/[0.03] px-3 py-2"
      transition:slide={{ duration: 300 }}
      data-testid="summarized-banner-content"
    >
      <p class="text-[11px] italic leading-relaxed text-muted-foreground/70">
        {summaryText}
      </p>
    </div>
  {/if}
</div>
