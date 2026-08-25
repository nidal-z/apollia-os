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
  import FileRef from "./FileRef.svelte";
  import { t } from "svelte-i18n";
  import { fly, fade } from "svelte/transition";

  type ToolCallItem = Extract<ReasoningItem, { kind: "tool_call" }>;

  interface Props {
    item: ToolCallItem;
    skin: "builder" | "operator";
    /** Session id, so a relative path resolves against the working directory. */
    sessionId?: string;
  }

  let { item, skin, sessionId }: Props = $props();

  const pattern = $derived(
    typeof item.args.pattern === "string" ? item.args.pattern : "",
  );
  const parsed = $derived(parseGrep(item.output));
  const rawJson = $derived(prettyJson(item.output));

  // Split a match line into hit / non-hit segments so the searched token can be
  // highlighted. The pattern is matched literally (case-insensitive); a complex
  // regex that does not match verbatim degrades to a single un-highlighted
  // segment rather than throwing.
  interface Segment {
    text: string;
    hit: boolean;
  }
  function escapeRegExp(s: string): string {
    return s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  }
  function highlightSegments(text: string, term: string): Segment[] {
    if (!term) return [{ text, hit: false }];
    let re: RegExp;
    try {
      re = new RegExp(escapeRegExp(term), "gi");
    } catch {
      return [{ text, hit: false }];
    }
    const out: Segment[] = [];
    let last = 0;
    let m: RegExpExecArray | null;
    while ((m = re.exec(text)) !== null) {
      if (m.index > last) out.push({ text: text.slice(last, m.index), hit: false });
      out.push({ text: m[0], hit: true });
      last = m.index + m[0].length;
      if (m[0].length === 0) re.lastIndex++;
    }
    if (last < text.length) out.push({ text: text.slice(last), hit: false });
    return out.length > 0 ? out : [{ text, hit: false }];
  }
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
            <p class="tb-metric">
              {$t("tools.body.grep_summary", {
                values: { m: parsed.totalMatches, f: parsed.filesSearched },
              })}{#if parsed.truncated}
                · {$t("tools.body.truncated_hint")}{/if}
            </p>
            <div class="tb-grep">
              {#each parsed.groups as group (group.file)}
                <div class="tb-grep-group">
                  <div class="tb-grep-file font-mono">
                    <FileRef path={group.file} label={basename(group.file)} {sessionId} />
                    {#if dirname(group.file)}
                      <span class="tb-grep-dir">{dirname(group.file)}</span>
                    {/if}
                  </div>
                  {#each group.rows as row, i (i)}
                    <div class="tb-grep-row font-mono">
                      <span class="tb-grep-ln"
                        >{$t("tools.body.line_marker", { values: { n: row.line } })}</span
                      >
                      <span class="tb-grep-text"
                        >{#each highlightSegments(row.text.trim(), pattern) as seg}{#if seg.hit}<span
                              class="tb-hit">{seg.text}</span
                            >{:else}{seg.text}{/if}{/each}</span
                      >
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
            {$t("chat.reasoning.output_label")}
          </div>
          <pre class="tb-code font-mono"><code>{rawJson || item.output}</code></pre>
        </div>
      {/if}
    </div>
  {/key}
</div>
