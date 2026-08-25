<script lang="ts">
  /**
   * Per-tool expanded body for `web_read`.
   *
   * Operator layer: the source URL as a chip plus a clean readable extract
   * (title, optional byline, the first lines of the article, and a character
   * count). Builder layer: the raw input (url) and the raw output JSON. A parse
   * failure degrades to the raw output so nothing is lost. The rich clickable
   * source card stays at the message level, untouched.
   */

  import type { ReasoningItem } from "$lib/chat/reasoning";
  import { parseWebReadOutput } from "$lib/chat/reasoning";
  import { stringifyValue, prettyJson } from "$lib/chat/toolBodies";
  import { handleExternalLinkClick } from "$lib/utils/externalLink";
  import { t, locale } from "svelte-i18n";
  import { fly, fade } from "svelte/transition";

  type ToolCallItem = Extract<ReasoningItem, { kind: "tool_call" }>;

  interface Props {
    item: ToolCallItem;
    skin: "builder" | "operator";
  }

  let { item, skin }: Props = $props();

  const EXTRACT_LINES = 8;

  const parsed = $derived(
    parseWebReadOutput({
      input: item.args,
      output: item.output,
      duration_ms: item.duration_ms,
    }),
  );

  const url = $derived(
    parsed?.url ?? (typeof item.args.url === "string" ? item.args.url : ""),
  );

  const extractLines = $derived(
    (parsed?.extracted ?? "")
      .split("\n")
      .map((l) => l.trim())
      .filter((l) => l.length > 0)
      .slice(0, EXTRACT_LINES),
  );

  const charsLabel = $derived.by<string>(() => {
    const n = parsed?.chars_total;
    if (typeof n !== "number") return "";
    const loc = $locale ?? "en";
    let formatted: string;
    try {
      formatted = new Intl.NumberFormat(loc).format(n);
    } catch {
      formatted = String(n);
    }
    const base = $t("tools.body.chars_count", { values: { n } });
    // Swap the plural sentence's own digits for the locale-grouped magnitude.
    return base.replace(String(n), formatted);
  });

  const inputJson = $derived(stringifyValue(item.args));
  const outputJson = $derived(prettyJson(item.output));
</script>

<div class="text-[12px]" in:fly={{ y: 8, duration: 200 }}>
  {#key skin}
    <div in:fade={{ duration: 150 }}>
      {#if skin === "operator"}
        <div class="flex flex-col gap-2">
          {#if url}
            <span class="tb-chip">
              <span class="tb-chip-key">{$t("tools.body.url_label")}</span>
              <a
                href={url}
                target="_blank"
                rel="noopener noreferrer"
                onclick={handleExternalLinkClick}
                class="tb-chip-val link-inline font-mono">{url}</a
              >
            </span>
          {/if}
          {#if parsed}
            <div class="tb-extract flex flex-col gap-1">
              {#if parsed.title}
                <p class="tb-extract-title">{parsed.title}</p>
              {/if}
              {#if parsed.byline}
                <p class="tb-extract-byline">
                  {$t("tools.body.byline_label")}
                  {parsed.byline}
                </p>
              {/if}
              {#if extractLines.length > 0}
                <p>{extractLines.join("\n")}</p>
              {/if}
            </div>
            <p class="tb-metric">
              {charsLabel}{#if parsed.truncated}
                · {$t("tools.body.truncated_hint")}{/if}
            </p>
          {:else if item.output}
            <pre class="tb-code font-mono"><code>{item.output}</code></pre>
          {/if}
        </div>
      {:else}
        <div class="flex flex-col gap-2">
          <div class="flex flex-col gap-1">
            <div class="tb-iolabel">
              {$t("chat.reasoning.input_label")}
            </div>
            <pre class="tb-code font-mono"><code>{inputJson}</code></pre>
          </div>
          {#if outputJson}
            <div class="flex flex-col gap-1">
              <div class="tb-iolabel">
                {$t("chat.reasoning.output_label")}
              </div>
              <pre class="tb-code font-mono"><code>{outputJson}</code></pre>
            </div>
          {/if}
        </div>
      {/if}
    </div>
  {/key}
</div>
