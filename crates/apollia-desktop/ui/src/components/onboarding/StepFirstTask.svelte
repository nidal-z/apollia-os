<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { Button } from "$lib/components/ui/button";
  import { tasks } from "$lib/stores/sse";
  import type { TaskSummary } from "$lib/types";

  interface Props {
    agentId: string;
    onFinish: () => void;
  }

  let { agentId, onFinish }: Props = $props();

  let inputText = $state("");
  let submitting = $state(false);
  let taskId = $state<string | null>(null);
  let error = $state<string | null>(null);

  let taskStatus = $derived.by(() => {
    if (!taskId) return null;
    const match = $tasks.find((t: TaskSummary) => t.id === taskId);
    return match ?? null;
  });

  let isCompleted = $derived(taskStatus?.status === "completed");
  let isFailed = $derived(taskStatus?.status === "failed");

  async function submitTask() {
    if (!inputText.trim()) return;

    submitting = true;
    error = null;
    try {
      const id = await invoke<string>("submit_task", {
        agentId,
        input: inputText.trim(),
      });
      taskId = id;
    } catch (err: unknown) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      submitting = false;
    }
  }
</script>

<div class="space-y-6">
  <div>
    <h2 class="text-xl font-semibold">Submit your first task</h2>
    <p class="mt-1 text-sm text-muted-foreground">
      Send a message to your agent and see the result.
    </p>
  </div>

  {#if !taskId}
    <div class="space-y-3">
      <textarea
        class="w-full rounded-lg border bg-background px-4 py-3 text-sm placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-ring"
        rows={4}
        placeholder="What would you like to ask your agent?"
        bind:value={inputText}
        disabled={submitting}
      ></textarea>
      <div class="flex justify-end">
        <Button onclick={submitTask} disabled={submitting || !inputText.trim()}>
          {submitting ? "Sending..." : "Send"}
        </Button>
      </div>
    </div>
  {/if}

  {#if error}
    <div class="rounded-lg border border-[hsl(var(--destructive))] bg-[hsl(var(--destructive))]/10 p-4">
      <p class="text-sm text-[hsl(var(--destructive))]">{error}</p>
    </div>
  {/if}

  {#if taskId && !isCompleted && !isFailed}
    <div class="flex items-center gap-3 rounded-lg border bg-card p-4">
      <span class="inline-block h-5 w-5 animate-spin rounded-full border-2 border-primary border-t-transparent"></span>
      <div>
        <p class="text-sm text-muted-foreground">
          Processing... {taskStatus?.status ?? "submitted"}
        </p>
        <p class="text-xs text-muted-foreground font-mono">{taskId}</p>
      </div>
    </div>
  {/if}

  {#if isCompleted}
    <div class="rounded-lg border border-[var(--apollia-success)] bg-[var(--apollia-success)]/10 p-4">
      <div class="flex items-center gap-2">
        <span class="text-lg text-[var(--apollia-success)]">&#10003;</span>
        <p class="text-sm font-medium">Task completed</p>
      </div>
      {#if taskStatus?.input_preview}
        <pre class="mt-3 whitespace-pre-wrap rounded bg-muted p-3 text-xs font-mono">{taskStatus.input_preview}</pre>
      {/if}
    </div>
  {/if}

  {#if isFailed}
    <div class="rounded-lg border border-[hsl(var(--destructive))] bg-[hsl(var(--destructive))]/10 p-4">
      <div class="flex items-center gap-2">
        <span class="text-lg text-[hsl(var(--destructive))]">&#10007;</span>
        <p class="text-sm font-medium">Task failed</p>
      </div>
    </div>
  {/if}

  <div class="flex justify-end gap-3">
    {#if isCompleted || isFailed}
      <Button onclick={onFinish}>Finish</Button>
    {/if}
  </div>
</div>
