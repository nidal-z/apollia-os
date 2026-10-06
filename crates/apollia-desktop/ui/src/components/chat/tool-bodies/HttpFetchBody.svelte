<script lang="ts">
  /**
   * Body of a call to a web address: the request, then the response (status,
   * size, time) and the opening of what it returned.
   */

  import type { ReasoningItem } from "$lib/chat/reasoning";
  import { parseHttp, humanSize } from "$lib/chat/toolBodies";
  import { formatSize, formatStepDuration } from "$lib/chat/toolBodyFormat";
  import BodyField from "./BodyField.svelte";
  import BodyCode from "./BodyCode.svelte";
  import { t, locale } from "svelte-i18n";

  type ToolCallItem = Extract<ReasoningItem, { kind: "tool_call" }>;

  let { item }: { item: ToolCallItem } = $props();

  /** Characters of the response shown at most. */
  const SHOWN = 800;

  const url = $derived(typeof item.args.url === "string" ? item.args.url : "");
  const method = $derived(
    typeof item.args.method === "string" ? item.args.method.toUpperCase() : "GET",
  );
  const parsed = $derived(parseHttp(item.output));

  const summary = $derived.by<string>(() => {
    if (!parsed) return "";
    const loc = $locale ?? "en";
    const parts = [$t("tools.body.response_status", { values: { status: parsed.status } })];
    if (parsed.byteSize != null) {
      const size = humanSize(parsed.byteSize);
      parts.push(formatSize(size.value, $t(`tools.body.unit_${size.unit}`), loc));
    }
    const duration = formatStepDuration(parsed.durationMs ?? item.duration_ms, loc);
    if (duration) parts.push($t("tools.body.in_duration", { values: { duration } }));
    return parts.join(", ");
  });

  const body = $derived.by<string>(() => {
    const raw = parsed?.body ?? "";
    let text = raw;
    try {
      text = JSON.stringify(JSON.parse(raw), null, 1).replace(/\n\s*/g, " ");
    } catch {
      text = raw;
    }
    return text.length > SHOWN ? `${text.slice(0, SHOWN - 1)}…` : text;
  });
</script>

<div class="flex flex-col gap-2" data-testid="tool-body-http_fetch">
  {#if url}
    <BodyField label={$t("tools.body.request_label")} value={`${method} ${url}`} mono />
  {/if}
  {#if summary}
    <p class="text-caption text-muted-foreground">{summary}</p>
  {/if}
  {#if body}
    <BodyCode text={body} />
  {/if}
</div>
