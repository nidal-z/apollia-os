<script lang="ts">
  /**
   * Per-tool expanded body for `todo_write`.
   *
   * Operator layer: the to-do list rendered cleanly, one row per item (status
   * glyph + text + status pill), parsed from `args.items` (the full list travels
   * there; the output only carries a count). Builder layer: the raw args and
   * output JSON. A missing / malformed list degrades to the raw output.
   */

  import type { ReasoningItem } from "$lib/chat/reasoning";
  import { parseTodos, stringifyValue, prettyJson } from "$lib/chat/toolBodies";
  import { t } from "svelte-i18n";
  import { fly, fade } from "svelte/transition";
  import { Circle, CircleDot, CheckCircle2 } from "lucide-svelte";

  type ToolCallItem = Extract<ReasoningItem, { kind: "tool_call" }>;

  interface Props {
    item: ToolCallItem;
    skin: "builder" | "operator";
  }

  let { item, skin }: Props = $props();

  const items = $derived(parseTodos(item.args));
  const doneCount = $derived(
    items ? items.filter((todo) => todo.status === "completed").length : 0,
  );

  function statusKey(status: string): string {
    switch (status) {
      case "in_progress":
        return "status_in_progress";
      case "completed":
        return "status_completed";
      default:
        return "status_pending";
    }
  }

  function pillClass(status: string): string {
    switch (status) {
      case "in_progress":
        return "tb-active";
      case "completed":
        return "tb-ok";
      default:
        return "";
    }
  }

  const inputJson = $derived(stringifyValue(item.args));
  const outputJson = $derived(prettyJson(item.output));
</script>

<div class="text-body-xs" in:fly={{ y: 8, duration: 200 }}>
  {#key skin}
    <div in:fade={{ duration: 150 }}>
      {#if skin === "operator"}
        <div class="flex flex-col gap-2">
          {#if items}
            <p class="tb-metric">
              {$t("tools.body.progress_fraction", {
                values: { done: doneCount, total: items.length },
              })} · {$t("tools.body.todo_count", { values: { n: items.length } })}
            </p>
            <div class="tb-steps">
              {#each items as todo (todo.id || todo.content)}
                <div class="tb-step">
                  <span class="tb-step-glyph {pillClass(todo.status)}" aria-hidden="true">
                    {#if todo.status === "completed"}
                      <CheckCircle2 size={14} />
                    {:else if todo.status === "in_progress"}
                      <CircleDot size={14} />
                    {:else}
                      <Circle size={14} />
                    {/if}
                  </span>
                  <span class="tb-step-title {todo.status === 'completed' ? 'tb-done' : ''}"
                    >{todo.content}</span
                  >
                  <span class="tb-pill tb-step-pill {pillClass(todo.status)}"
                    >{$t(`tools.body.${statusKey(todo.status)}`)}</span
                  >
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
