<script lang="ts">
  /**
   * Per-tool expanded body for `http_fetch`.
   *
   * Operator layer: a "{method} {url}" chip, a coloured status badge
   * ("200 Success", success / warn / error by class) and a short summary
   * (content-type and size when present). Builder layer: the raw request
   * (method, url, args) and the raw response (status, headers, body) as JSON /
   * terminal blocks. A parse failure degrades to the raw output.
   */

  import type { ReasoningItem } from "$lib/chat/reasoning";
  import {
    parseHttp,
    httpStatusClass,
    humanSize,
    stringifyValue,
    prettyJson,
    type HumanSize,
  } from "$lib/chat/toolBodies";
  import { handleExternalLinkClick } from "$lib/utils/externalLink";
  import { t, locale } from "svelte-i18n";
  import { fly, fade } from "svelte/transition";

  type ToolCallItem = Extract<ReasoningItem, { kind: "tool_call" }>;

  interface Props {
    item: ToolCallItem;
    skin: "builder" | "operator";
  }

  let { item, skin }: Props = $props();

  const url = $derived(typeof item.args.url === "string" ? item.args.url : "");
  const method = $derived(
    typeof item.args.method === "string"
      ? item.args.method.toUpperCase()
      : "GET",
  );
  const parsed = $derived(parseHttp(item.output));

  const statusMeta = $derived.by(() => {
    if (!parsed) return null;
    const cls = httpStatusClass(parsed.status);
    const badge =
      cls === "success"
        ? "tb-ok"
        : cls === "client"
          ? "tb-warn"
          : cls === "server" || cls === "unknown"
            ? "tb-error"
            : "tb-info";
    const label = $t(`tools.body.http_status_${cls}`);
    return { badge, label };
  });

  function formatSize(size: HumanSize): string {
    const loc = $locale ?? "en";
    let n: string;
    try {
      n = new Intl.NumberFormat(loc, { maximumFractionDigits: 1 }).format(
        size.value,
      );
    } catch {
      n = String(size.value);
    }
    return `${n} ${$t(`tools.body.unit_${size.unit}`)}`;
  }

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
              <span class="tb-chip-key font-mono">{method}</span>
              <a
                href={url}
                target="_blank"
                rel="noopener noreferrer"
                onclick={handleExternalLinkClick}
                class="tb-chip-val link-inline font-mono">{url}</a
              >
            </span>
          {/if}
          {#if parsed && statusMeta}
            <div class="flex flex-wrap items-center gap-2">
              <span class="tb-badge {statusMeta.badge}">
                {parsed.status} {statusMeta.label}
              </span>
              {#if parsed.contentType}
                <span class="tb-metric">
                  {$t("tools.body.content_type_label")}: {parsed.contentType}
                </span>
              {/if}
              {#if parsed.byteSize != null}
                <span class="tb-metric">
                  {$t("tools.body.size_label")}: {formatSize(humanSize(parsed.byteSize))}
                </span>
              {/if}
            </div>
          {:else if item.output}
            <pre class="tb-code font-mono"><code>{item.output}</code></pre>
          {/if}
        </div>
      {:else}
        <div class="flex flex-col gap-2">
          <div class="flex flex-col gap-1">
            <div class="tb-iolabel">{$t("tools.body.request_label")}</div>
            <pre class="tb-code font-mono"><code>{inputJson}</code></pre>
          </div>
          {#if outputJson}
            <div class="flex flex-col gap-1">
              <div class="tb-iolabel">{$t("tools.body.response_label")}</div>
              <pre class="tb-code font-mono"><code>{outputJson}</code></pre>
            </div>
          {/if}
        </div>
      {/if}
    </div>
  {/key}
</div>
