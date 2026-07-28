<script lang="ts">
  /**
   * HelpFaq - searchable frequently-asked-questions list.
   *
   * Each entry reuses the Phase-0 `Disclosure` primitive (animated body, chevron
   * rotation, `aria-expanded`, reduced-motion aware). The search field filters
   * the frozen entries (question + answer) live; Escape clears the query. An
   * empty query shows everything, a no-match query shows a dedicated empty
   * state rather than a silent blank list.
   */
  import { t } from "svelte-i18n";
  import { Search } from "lucide-svelte";
  import { Input } from "$lib/components/ui/input";
  import { Disclosure } from "$lib/components/ui/disclosure";

  interface FaqItem {
    id: string;
    question: string;
    answer: string;
  }

  interface Props {
    items: FaqItem[];
  }

  let { items }: Props = $props();

  let query = $state("");

  const filtered = $derived.by<FaqItem[]>(() => {
    const q = query.trim().toLowerCase();
    if (q === "") return items;
    return items.filter((item) =>
      `${item.question} ${item.answer}`.toLowerCase().includes(q),
    );
  });

  function onKeydown(event: KeyboardEvent): void {
    if (event.key === "Escape" && query !== "") {
      event.preventDefault();
      query = "";
    }
  }
</script>

<div class="space-y-3" data-testid="help-faq">
  <Input
    type="search"
    icon={Search}
    bind:value={query}
    onkeydown={onKeydown}
    placeholder={$t("settings.help.faq_search_placeholder")}
    aria-label={$t("settings.help.faq_search_aria")}
    data-testid="help-faq-search"
  />

  {#if filtered.length === 0}
    <div
      class="flex flex-col items-center gap-2.5 rounded-xl border border-dashed border-border
        bg-surface-2/40 px-5 py-9 text-center"
      data-testid="help-faq-empty"
    >
      <span
        class="grid h-11 w-11 place-items-center rounded-full bg-muted text-muted-foreground"
      >
        <Search size={20} strokeWidth={1.75} aria-hidden="true" />
      </span>
      <p class="text-body-sm font-medium text-foreground">
        {$t("settings.help.faq_empty_title")}
      </p>
      <p class="max-w-sm text-body-xs text-muted-foreground">
        {$t("settings.help.faq_empty_body", { values: { query: query.trim() } })}
      </p>
    </div>
  {:else}
    <div class="space-y-2">
      {#each filtered as item (item.id)}
        <Disclosure testid="help-faq-item-{item.id}">
          {#snippet summary()}
            <span class="text-body-sm font-medium text-foreground">{item.question}</span>
          {/snippet}
          {#snippet children()}
            <p class="text-body-xs leading-relaxed text-muted-foreground">{item.answer}</p>
          {/snippet}
        </Disclosure>
      {/each}
    </div>
  {/if}
</div>
