<script lang="ts">
  /**
   * Body of a page read: the page by its title and address, how much text was
   * read, and the opening of what it says.
   */

  import type { ReasoningItem } from "$lib/chat/reasoning";
  import { parseWebReadOutput } from "$lib/chat/reasoning";
  import { hostOf } from "$lib/chat/toolStep";
  import { handleExternalLinkClick } from "$lib/utils/externalLink";
  import { t, locale } from "svelte-i18n";
  import { BookOpen } from "lucide-svelte";
  import BodyRow from "./BodyRow.svelte";

  type ToolCallItem = Extract<ReasoningItem, { kind: "tool_call" }>;

  let { item }: { item: ToolCallItem } = $props();

  /** Characters of the page quoted at most. */
  const QUOTE_MAX = 280;

  const parsed = $derived(
    parseWebReadOutput({ input: item.args, output: item.output, duration_ms: item.duration_ms }),
  );
  const url = $derived(parsed?.url ?? (typeof item.args.url === "string" ? item.args.url : ""));
  const shownUrl = $derived(url.replace(/^https?:\/\/(www\.)?/, "").replace(/\/$/, ""));

  const quote = $derived.by<string>(() => {
    const text = (parsed?.extracted ?? "").replace(/\s+/g, " ").trim();
    return text.length > QUOTE_MAX ? `${text.slice(0, QUOTE_MAX - 1)}…` : text;
  });

  const charsRead = $derived.by<string>(() => {
    const n = parsed?.chars_total;
    if (typeof n !== "number") return "";
    let grouped: string;
    try {
      grouped = new Intl.NumberFormat($locale ?? "en").format(n);
    } catch {
      grouped = String(n);
    }
    const count = $t("tools.body.chars_count", { values: { n } }).replace(String(n), grouped);
    return $t("tools.body.text_read", { values: { count } });
  });
</script>

<div class="flex flex-col gap-2" data-testid="tool-body-web_read">
  {#if url}
    <BodyRow icon={BookOpen} sub={shownUrl}>
      <a
        href={url}
        target="_blank"
        rel="noopener noreferrer"
        onclick={handleExternalLinkClick}
        class="hover:underline">{parsed?.title || hostOf(url)}</a
      >
    </BodyRow>
  {/if}
  {#if charsRead}
    <p class="text-caption text-muted-foreground">
      {charsRead}{#if parsed?.truncated}, {$t("tools.body.truncated_hint")}{/if}
    </p>
  {/if}
  {#if quote}
    <p class="text-body-xs text-foreground">{$t("tools.body.quoted", { values: { text: quote } })}</p>
  {:else if !parsed && item.output}
    <p class="text-body-xs text-muted-foreground">{item.output.slice(0, QUOTE_MAX)}</p>
  {/if}
</div>
