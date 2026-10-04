<script lang="ts">
  /**
   * Body of the agent's task list: how many tasks are done, then each task
   * with its state.
   */

  import type { ReasoningItem } from "$lib/chat/reasoning";
  import { parseTodos } from "$lib/chat/toolBodies";
  import BodyRow from "./BodyRow.svelte";
  import { t } from "svelte-i18n";
  import { Check, CircleDot, ListTodo } from "lucide-svelte";

  type ToolCallItem = Extract<ReasoningItem, { kind: "tool_call" }>;

  let { item }: { item: ToolCallItem } = $props();

  const todos = $derived(parseTodos(item.args));
  const done = $derived(todos ? todos.filter((todo) => todo.status === "completed").length : 0);

  function iconOf(status: string) {
    if (status === "completed") return Check;
    if (status === "in_progress") return CircleDot;
    return ListTodo;
  }
</script>

<div class="flex flex-col gap-2" data-testid="tool-body-todo_write">
  {#if todos}
    <p class="text-caption text-muted-foreground">
      {$t("tools.body.todos_done", { values: { done, total: todos.length } })}
    </p>
    {#each todos as todo (todo.id || todo.content)}
      <BodyRow icon={iconOf(todo.status)} title={todo.content} />
    {/each}
  {:else if item.output}
    <p class="text-body-xs text-muted-foreground">{item.output.slice(0, 400)}</p>
  {/if}
</div>
