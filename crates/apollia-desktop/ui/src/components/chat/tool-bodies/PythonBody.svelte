<script lang="ts">
  /**
   * Per-tool expanded body for `python_executor`.
   *
   * Mirrors `BashBody`: a shared code-executor presentation reusing the
   * `.tb-term` terminal primitive. Operator layer: a plain-language outcome line
   * (the meta-LLM narration when present, else a generic localized sentence)
   * plus a status/line-count meta row, with no code shown. Builder layer: a
   * terminal block with the source code, stdout/stderr, and a localized
   * exit/status + duration footer. The runtime serializes the output as
   * `{stdout, stderr, exit_code, duration_ms}`, which `parseBashOutput` already
   * normalizes.
   */

  import type { ReasoningItem } from "$lib/chat/reasoning";
  import { parseBashOutput, countOutputLines } from "$lib/chat/toolBodies";
  import { t, locale } from "svelte-i18n";
  import { fly, fade } from "svelte/transition";

  type ToolCallItem = Extract<ReasoningItem, { kind: "tool_call" }>;

  interface Props {
    item: ToolCallItem;
    skin: "builder" | "operator";
  }

  let { item, skin }: Props = $props();

  const code = $derived(typeof item.args.code === "string" ? item.args.code : "");
  const parsed = $derived(parseBashOutput(item.output, item.exit_code));
  const succeeded = $derived(
    (parsed.exitCode === null || parsed.exitCode === 0) &&
      item.status !== "error" &&
      item.status !== "rejected",
  );
  const lineCount = $derived(countOutputLines(item.output));

  const outcome = $derived(
    item.rationale?.summary?.trim()
      ? item.rationale.summary.trim()
      : $t("tools.body.python_generic"),
  );

  const durationLabel = $derived.by<string>(() => {
    const ms = item.duration_ms;
    if (ms == null || ms <= 0) return "";
    if (ms < 1000) return `${Math.round(ms)} ms`;
    const loc = $locale ?? "en";
    try {
      return `${new Intl.NumberFormat(loc, {
        minimumFractionDigits: 1,
        maximumFractionDigits: 1,
      }).format(ms / 1000)} s`;
    } catch {
      return `${(ms / 1000).toFixed(1)} s`;
    }
  });

  const statusLabel = $derived(
    succeeded
      ? $t("tools.body.bash_ok")
      : parsed.exitCode != null
        ? $t("tools.body.bash_failed_code", {
            values: { code: parsed.exitCode },
          })
        : $t("tools.body.bash_failed"),
  );
</script>

<div class="text-[12px]" in:fly={{ y: 8, duration: 200 }}>
  {#key skin}
    <div in:fade={{ duration: 150 }}>
      {#if skin === "operator"}
        <div class="flex flex-col gap-1">
          <p class="text-foreground">{outcome}</p>
          <p class="tb-metric">
            {statusLabel}{#if lineCount > 0}
              · {$t("tools.body.output_lines", { values: { n: lineCount } })}{/if}
          </p>
        </div>
      {:else}
        <pre class="tb-term font-mono"><span class="tb-prompt">›</span> {code}{#if parsed.body}
<span class="tb-out">{parsed.body.replace(/\n+$/, "")}</span>{/if}{#if parsed.stderr}
<span class="tb-err">{parsed.stderr.replace(/\n+$/, "")}</span>{/if}
<span
            class={succeeded ? "tb-exit" : "tb-exit-nonzero"}
          >{parsed.exitCode != null
              ? $t("tools.body.exit_code", { values: { code: parsed.exitCode } })
              : statusLabel}</span>{#if durationLabel}<span class="tb-meta"> · {durationLabel}</span>{/if}</pre>
      {/if}
    </div>
  {/key}
</div>
