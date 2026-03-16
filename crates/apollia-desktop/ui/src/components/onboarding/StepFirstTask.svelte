<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { Button } from "$lib/components/ui/button";
  import { tasks, refreshAll } from "$lib/stores/sse";
  import MacSandboxBanner from "../common/MacSandboxBanner.svelte";
  import type { TaskSummary, PendingApproval } from "$lib/types";

  interface Props {
    agentId: string;
    onFinish: () => void;
  }

  let { agentId, onFinish }: Props = $props();

  let inputText = $state("");
  let submitting = $state(false);
  let taskId = $state<string | null>(null);
  let error = $state<string | null>(null);

  // Live timeline events polled every 1s while task is active.
  let events = $state<any[]>([]);
  let pendingApproval = $state<PendingApproval | null>(null);
  let rejectReason = $state("");
  let showRejectInput = $state(false);
  let resuming = $state(false);

  let pollTimer: ReturnType<typeof setInterval> | null = null;

  let taskStatus = $derived.by(() => {
    if (!taskId) return null;
    return $tasks.find((t: TaskSummary) => t.id === taskId) ?? null;
  });

  let isCompleted = $derived(taskStatus?.status === "completed");
  let isFailed = $derived(taskStatus?.status === "failed");
  // input_required is detected from the tasks store (polled every 3s) OR
  // from the timeline events (polled every 1s) — whichever fires first.
  let hitlFromTimeline = $derived(
    events.some((e) => e.type === "hitl_suspended") &&
    !events.some((e) => e.type === "hitl_resolved")
  );
  let isInputRequired = $derived(
    taskStatus?.status === "input_required" || hitlFromTimeline
  );
  let isTerminal = $derived(isCompleted || isFailed);

  // ─── Polling ─────────────────────────────────────────────────────────────────

  function startPolling() {
    if (pollTimer) return;
    pollTimer = setInterval(() => void tick(), 1000);
  }

  function stopPolling() {
    if (pollTimer) {
      clearInterval(pollTimer);
      pollTimer = null;
    }
  }

  async function tick(): Promise<void> {
    if (!taskId) return;
    if (isTerminal) {
      stopPolling();
      return;
    }

    try {
      const result = await invoke<any[]>("get_task_timeline", { taskId });
      events = result;
    } catch {
      // Keep current events on transient error
    }

    // Fetch HITL approval details when task is suspended.
    if (isInputRequired && !pendingApproval) {
      try {
        const approvals = await invoke<PendingApproval[]>("list_pending_approvals");
        const match = approvals.find((a) => a.task_id === taskId);
        if (match) pendingApproval = match;
      } catch {
        // Ignore transient error
      }
    }
  }

  // Stop polling when component is destroyed.
  $effect(() => () => stopPolling());

  // ─── Actions ─────────────────────────────────────────────────────────────────

  async function submitTask(): Promise<void> {
    if (!inputText.trim()) return;
    submitting = true;
    error = null;
    try {
      const id = await invoke<string>("submit_task", {
        agentId,
        input: inputText.trim(),
      });
      taskId = id;
      startPolling();
    } catch (err: unknown) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      submitting = false;
    }
  }

  async function approve(): Promise<void> {
    if (!taskId) return;
    resuming = true;
    error = null;
    try {
      await invoke("resume_task", { taskId, approved: true, reason: null });
      pendingApproval = null;
      showRejectInput = false;
      await refreshAll();
      startPolling();
    } catch (err: unknown) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      resuming = false;
    }
  }

  async function reject(): Promise<void> {
    if (!taskId || !rejectReason.trim()) return;
    resuming = true;
    error = null;
    try {
      await invoke("resume_task", {
        taskId,
        approved: false,
        reason: rejectReason.trim(),
      });
      pendingApproval = null;
      showRejectInput = false;
      rejectReason = "";
      await refreshAll();
      // Task will be failed after rejection — no need to poll.
    } catch (err: unknown) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      resuming = false;
    }
  }

  // ─── Event display helpers ────────────────────────────────────────────────────

  function eventIcon(type: string): string {
    switch (type) {
      case "task_transition": return "↪";
      case "llm_call":        return "✦";
      case "tool_call":       return "⚙";
      case "step_started":    return "▶";
      case "step_completed":  return "✓";
      case "hitl_suspended":  return "⏸";
      case "hitl_resolved":   return "▶";
      case "task_completed":  return "✓";
      default:                return "·";
    }
  }

  function eventColor(type: string): string {
    switch (type) {
      case "llm_call":        return "text-blue-400";
      case "tool_call":       return "text-amber-400";
      case "hitl_suspended":  return "text-orange-400";
      case "hitl_resolved":   return "text-green-400";
      case "task_completed":  return "text-[var(--apollia-success)]";
      case "task_transition": return "text-muted-foreground";
      default:                return "text-muted-foreground";
    }
  }

  function eventSummary(ev: any): string {
    switch (ev.type) {
      case "task_transition":
        return `Status → ${ev.status}`;
      case "llm_call": {
        const tokens = (ev.prompt_tokens ?? 0) + (ev.completion_tokens ?? 0);
        const lat = ev.latency_ms ? `${ev.latency_ms}ms` : "";
        return `LLM ${ev.model ?? ev.backend} — ${tokens} tokens${lat ? ` · ${lat}` : ""}`;
      }
      case "tool_call": {
        const ok = ev.exit_code === 0 ? "✓" : "✗";
        const dur = ev.duration_ms ? ` · ${ev.duration_ms}ms` : "";
        return `${ev.tool_name} ${ok}${dur}`;
      }
      case "step_started":
        return `Step: ${ev.tool ?? ev.step_id ?? ""}`;
      case "step_completed":
        return `Step ${ev.success ? "OK" : "failed"}${ev.duration_ms ? ` · ${ev.duration_ms}ms` : ""}`;
      case "hitl_suspended":
        return `Approval required: ${ev.prompt}`;
      case "hitl_resolved":
        return `${ev.approved ? "Approved" : "Rejected"}${ev.reason ? ` — ${ev.reason}` : ""}`;
      case "task_completed":
        return `Completed${ev.duration_ms ? ` · ${ev.duration_ms}ms` : ""}`;
      default:
        return ev.type ?? "event";
    }
  }

  function shortTime(iso: string): string {
    if (!iso) return "";
    try {
      return new Date(iso).toLocaleTimeString(undefined, {
        hour: "2-digit",
        minute: "2-digit",
        second: "2-digit",
      });
    } catch {
      return iso.slice(11, 19);
    }
  }
</script>

<div class="space-y-6">
  <div>
    <h2 class="text-xl font-semibold">Submit your first task</h2>
    <p class="mt-1 text-sm text-muted-foreground">
      Send a message to your agent and watch it run in real time.
    </p>
  </div>

  <MacSandboxBanner />

  <!-- Input form — hidden once a task is running -->
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

  <!-- Live activity panel — shown while task is running or complete -->
  {#if taskId}
    <div class="rounded-lg border bg-card overflow-hidden">
      <!-- Header -->
      <div class="flex items-center justify-between gap-3 px-4 py-3 border-b bg-muted/30">
        <div class="flex items-center gap-2">
          {#if !isTerminal}
            <span class="inline-block h-2 w-2 animate-pulse rounded-full bg-primary"></span>
          {:else if isCompleted}
            <span class="h-2 w-2 rounded-full bg-[var(--apollia-success)]"></span>
          {:else}
            <span class="h-2 w-2 rounded-full bg-[hsl(var(--destructive))]"></span>
          {/if}
          <span class="text-xs font-medium">
            {#if isInputRequired}
              Waiting for approval
            {:else if isTerminal}
              {isCompleted ? "Completed" : "Failed"}
            {:else}
              Running — {taskStatus?.status ?? "submitted"}
            {/if}
          </span>
        </div>
        <span class="font-mono text-xs text-muted-foreground">{taskId.slice(0, 8)}</span>
      </div>

      <!-- HITL approval card — shown when task is suspended -->
      {#if isInputRequired && pendingApproval}
        <div class="px-4 py-4 border-b bg-amber-500/5 border-amber-500/20">
          <div class="flex items-start gap-3">
            <span class="text-xl text-amber-400 leading-none mt-0.5">⏸</span>
            <div class="flex-1 space-y-3">
              <div>
                <p class="text-sm font-semibold text-amber-300">Agent approval required</p>
                <p class="mt-1 text-sm text-muted-foreground">{pendingApproval.prompt}</p>
              </div>
              {#if showRejectInput}
                <div class="space-y-2">
                  <input
                    class="w-full rounded border bg-background px-3 py-2 text-sm placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-ring"
                    placeholder="Reason for rejection (required)"
                    bind:value={rejectReason}
                    disabled={resuming}
                  />
                  <div class="flex gap-2">
                    <Button
                      variant="destructive"
                      size="sm"
                      onclick={reject}
                      disabled={resuming || !rejectReason.trim()}
                    >
                      {resuming ? "Rejecting..." : "Confirm rejection"}
                    </Button>
                    <Button
                      variant="ghost"
                      size="sm"
                      onclick={() => { showRejectInput = false; rejectReason = ""; }}
                      disabled={resuming}
                    >
                      Cancel
                    </Button>
                  </div>
                </div>
              {:else}
                <div class="flex gap-2">
                  <Button size="sm" onclick={approve} disabled={resuming}>
                    {resuming ? "Approving..." : "Approve"}
                  </Button>
                  <Button
                    variant="outline"
                    size="sm"
                    onclick={() => { showRejectInput = true; }}
                    disabled={resuming}
                  >
                    Reject
                  </Button>
                </div>
              {/if}
            </div>
          </div>
        </div>
      {:else if isInputRequired}
        <!-- Approval data loading -->
        <div class="px-4 py-3 border-b bg-amber-500/5">
          <div class="flex items-center gap-2">
            <span class="inline-block h-4 w-4 animate-spin rounded-full border-2 border-amber-400 border-t-transparent"></span>
            <span class="text-xs text-amber-300">Loading approval details...</span>
          </div>
        </div>
      {/if}

      <!-- Timeline event log -->
      <div class="max-h-64 overflow-y-auto">
        {#if events.length === 0}
          <div class="flex items-center gap-2 px-4 py-4 text-xs text-muted-foreground">
            <span class="inline-block h-3 w-3 animate-spin rounded-full border-2 border-muted-foreground border-t-transparent"></span>
            Waiting for events...
          </div>
        {:else}
          <ul class="py-1">
            {#each events as ev, i (i)}
              <li class="flex items-start gap-3 px-4 py-1.5 hover:bg-muted/20 transition-colors">
                <span class="mt-0.5 shrink-0 text-xs font-mono {eventColor(ev.type)}">{eventIcon(ev.type)}</span>
                <div class="flex-1 min-w-0">
                  <p class="text-xs leading-relaxed text-foreground truncate">
                    {eventSummary(ev)}
                  </p>
                </div>
                <span class="shrink-0 text-xs text-muted-foreground/60 font-mono tabular-nums">
                  {shortTime(ev.timestamp)}
                </span>
              </li>
            {/each}
          </ul>
        {/if}
      </div>
    </div>
  {/if}

  <!-- Terminal state actions -->
  <div class="flex items-center justify-between">
    {#if isCompleted}
      <p class="text-sm text-[var(--apollia-success)] font-medium">
        ✓ Task completed successfully
      </p>
      <Button onclick={onFinish}>Finish setup</Button>
    {:else if isFailed}
      <p class="text-sm text-[hsl(var(--destructive))] font-medium">
        Task ended with an error
      </p>
      <Button variant="outline" onclick={onFinish}>Continue anyway</Button>
    {:else}
      <div></div>
    {/if}
  </div>
</div>
