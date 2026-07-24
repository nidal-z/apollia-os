<script lang="ts">
  /**
   * Per-tool expanded body for `file_grep`.
   *
   * Operator layer: the search pattern as a chip, a "{matches} in {files}" lead
   * line, then matches grouped by file (basename header + parent dir, then one
   * row per match, "L{line}  {text}"). Builder layer: the raw output JSON. A
   * parse failure degrades to the raw string.
   */

  import type { ReasoningItem } from "$lib/chat/reasoning";
  import { parseGrep, basename, dirname, prettyJson } from "$lib/chat/toolBodies";
  import { t } from "svelte-i18n";
  import { fly, fade } from "svelte/transition";

  type ToolCallItem = Extract<ReasoningItem, { kind: "tool_call" }>;

  interface Props {
    item: ToolCallItem;
    skin: "builder" | "operator";
  }

  let { item, skin }: Props = $props();

  const pattern = $derived(
    typeof item.args.pattern === "string" ? item.args.pattern : "",
  );
  const parsed = $derived(parseGrep(item.output));
  const rawJson = $derived(prettyJson(item.output));
</script>

<div class="text-[12px]" in:fly={{ y: 8, duration: 200 }}>
  {#key skin}
    <div in:fade={{ duration: 150 }}>
      {#if skin === "operator"}
        <div class="flex flex-col gap-2">
          {#if pattern}
            <span class="tb-chip">
              <span class="tb-chip-key">{$t("tools.body.pattern_label")}</span>
              <span class="tb-chip-val font-mono">{pattern}</span>
            </span>
          {/if}
          {#if parsed}
            <p class="text-[11px] text-muted-foreground">
              {$t("tools.body.grep_summary", {
                values: { m: parsed.totalMatches, f: parsed.filesSearched },
              })}{#if parsed.truncated}
                · {$t("tools.body.truncated_hint")}{/if}
            </p>
            <div class="tb-grep">
              {#each parsed.groups as group (group.file)}
                <div class="tb-grep-group">
                  <div class="tb-grep-file font-mono">
                    <span>{basename(group.file)}</span>
                    {#if dirname(group.file)}
                      <span class="tb-grep-dir">{dirname(group.file)}</span>
                    {/if}
                  </div>
                  {#each group.rows as row, i (i)}
                    <div class="tb-grep-row font-mono">
                      <span class="tb-grep-ln"
                        >{$t("tools.body.line_marker", { values: { n: row.line } })}</span
                      >
                      <span class="tb-grep-text">{row.text.trim()}</span>
                    </div>
                  {/each}
                </div>
              {/each}
            </div>
          {:else if item.output}
            <pre class="tb-code font-mono"><code>{item.output}</code></pre>
          {/if}
        </div>
      {:else}
        <div class="flex flex-col gap-1">
          <div class="tb-iolabel">
            {$t("chat.reasoning.output_label", { default: "Output" })}
          </div>
          <pre class="tb-code font-mono"><code>{rawJson || item.output}</code></pre>
        </div>
      {/if}
    </div>
  {/key}
</div>
