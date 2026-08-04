<script lang="ts">
  /**
   * Per-tool expanded body for `file_glob`.
   *
   * Operator layer: the glob pattern as a chip, then the matched paths as a
   * clean listing (basename in the foreground, parent directory muted). Builder
   * layer: the raw input and output JSON. A parse failure degrades to the raw
   * string.
   */

  import type { ReasoningItem } from "$lib/chat/reasoning";
  import { parseFileGlob, basename, dirname, prettyJson } from "$lib/chat/toolBodies";
  import FileRef from "./FileRef.svelte";
  import { t } from "svelte-i18n";
  import { fly, fade } from "svelte/transition";
  import { FileText } from "lucide-svelte";

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
  const matches = $derived(parseFileGlob(item.output));
  const rawJson = $derived(prettyJson(item.output));
  const inputJson = $derived(JSON.stringify(item.args, null, 2));
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
          {#if matches}
            <p class="tb-metric">
              {$t("tools.body.matches_count", { values: { n: matches.length } })}
            </p>
            <div class="tb-flist">
              {#each matches as path (path)}
                <div class="tb-frow">
                  <span class="tb-fi" aria-hidden="true"><FileText size={14} /></span>
                  <FileRef {path} label={basename(path)} class="tb-fname" {sessionId} />
                  {#if dirname(path)}
                    <span class="tb-fmeta font-mono">{dirname(path)}</span>
                  {/if}
                </div>
              {/each}
            </div>
          {:else if item.output}
            <pre class="tb-code font-mono"><code>{item.output}</code></pre>
          {/if}
        </div>
      {:else}
        <div class="flex flex-col gap-2">
          <div class="flex flex-col gap-1">
            <div class="tb-iolabel">
              {$t("chat.reasoning.input_label", { default: "Input" })}
            </div>
            <pre class="tb-code font-mono"><code>{inputJson}</code></pre>
          </div>
          <div class="flex flex-col gap-1">
            <div class="tb-iolabel">
              {$t("chat.reasoning.output_label", { default: "Output" })}
            </div>
            <pre class="tb-code font-mono"><code>{rawJson || item.output}</code></pre>
          </div>
        </div>
      {/if}
    </div>
  {/key}
</div>
