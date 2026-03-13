<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onDestroy } from "svelte";
  import type { PendingApproval } from "$lib/types";
  import {
    Card,
    CardHeader,
    CardTitle,
    CardContent,
  } from "$lib/components/ui/card";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";

  interface Props {
    approval: PendingApproval;
  }

  let { approval }: Props = $props();

  const WARNING_THRESHOLD_MS = 1_800_000;
  const MIN_REASON_LENGTH = 10;

  // Live elapsed counter
  let elapsedMs = $state(computeElapsed());
  const timer = setInterval(() => {
    elapsedMs = computeElapsed();
  }, 1_000);

  onDestroy(() => {
    clearInterval(timer);
  });

  function computeElapsed(): number {
    if (!approval.suspended_at) return 0;
    return Math.max(0, Date.now() - new Date(approval.suspended_at).getTime());
  }

  function formatElapsed(ms: number): string {
    const totalSecs = Math.floor(ms / 1_000);
    const mins = Math.floor(totalSecs / 60);
    const secs = totalSecs % 60;
    if (mins > 0) return `${mins}m ${secs}s`;
    return `${secs}s`;
  }

  let isOverThreshold = $derived(elapsedMs > WARNING_THRESHOLD_MS);

  // Approve flow
  let showApproveConfirm = $state(false);
  let approving = $state(false);
  let approveError = $state<string | null>(null);

  // Reject flow
  let showRejectDialog = $state(false);
  let rejectReason = $state("");
  let rejecting = $state(false);
  let rejectError = $state<string | null>(null);
  let reasonValid = $derived(rejectReason.trim().length >= MIN_REASON_LENGTH);

  // Context JSON toggle
  let showContext = $state(false);

  // Resolved state — hide card after action
  let resolved = $state(false);

  async function handleApprove() {
    approving = true;
    approveError = null;
    try {
      await invoke("resume_task", {
        taskId: approval.task_id,
        approved: true,
        reason: null,
      });
      resolved = true;
    } catch (err: unknown) {
      approveError = err instanceof Error ? err.message : String(err);
    } finally {
      approving = false;
      showApproveConfirm = false;
    }
  }

  async function handleReject() {
    if (!reasonValid) return;
    rejecting = true;
    rejectError = null;
    try {
      await invoke("resume_task", {
        taskId: approval.task_id,
        approved: false,
        reason: rejectReason.trim(),
      });
      resolved = true;
    } catch (err: unknown) {
      rejectError = err instanceof Error ? err.message : String(err);
    } finally {
      rejecting = false;
    }
  }

  function openApproveConfirm() {
    showApproveConfirm = true;
    showRejectDialog = false;
  }

  function openRejectDialog() {
    showRejectDialog = true;
    showApproveConfirm = false;
    rejectReason = "";
    rejectError = null;
  }

  function cancelDialogs() {
    showApproveConfirm = false;
    showRejectDialog = false;
    rejectReason = "";
  }

  function shortId(id: string): string {
    return id.slice(0, 8);
  }
</script>

{#if !resolved}
  <Card data-testid="approval-card" data-task-id={approval.task_id}>
    <CardHeader class="pb-2">
      <div class="flex items-center justify-between">
        <div class="flex items-center gap-2">
          <CardTitle class="text-base font-semibold">
            {approval.agent_name || "Unknown agent"}
          </CardTitle>
          <code class="text-xs text-muted-foreground">{shortId(approval.task_id)}</code>
        </div>
        <Badge
          variant="outline"
          class={isOverThreshold
            ? "border-[hsl(var(--destructive))] text-[hsl(var(--destructive))]"
            : "border-[var(--apollia-warning)] text-[var(--apollia-warning)]"}
        >
          {formatElapsed(elapsedMs)}
        </Badge>
      </div>
    </CardHeader>

    <CardContent>
      <div class="space-y-3">
        <!-- Prompt -->
        <div>
          <h4 class="mb-1 text-xs font-semibold text-muted-foreground">Prompt</h4>
          <div class="max-h-[400px] overflow-auto rounded border bg-muted/30 p-3">
            <p class="whitespace-pre-wrap text-sm">{approval.prompt || "No prompt"}</p>
          </div>
        </div>

        <!-- Context JSON (collapsible) -->
        {#if approval.context}
          <div>
            <button
              class="flex items-center gap-1 text-xs font-semibold text-muted-foreground hover:text-foreground"
              onclick={() => (showContext = !showContext)}
            >
              <span>{showContext ? "Hide" : "Show"} context</span>
              <span class="text-[10px]">{showContext ? "▲" : "▼"}</span>
            </button>
            {#if showContext}
              <div class="mt-1 max-h-[300px] overflow-auto rounded border bg-muted/30 p-3">
                <pre class="text-xs">{JSON.stringify(approval.context, null, 2)}</pre>
              </div>
            {/if}
          </div>
        {/if}

        <!-- Error messages -->
        {#if approveError}
          <p class="text-xs text-[hsl(var(--destructive))]">{approveError}</p>
        {/if}
        {#if rejectError}
          <p class="text-xs text-[hsl(var(--destructive))]">{rejectError}</p>
        {/if}

        <!-- Action buttons / dialogs -->
        {#if showApproveConfirm}
          <div class="rounded border border-[var(--apollia-success)] bg-[var(--apollia-success)]/5 p-3">
            <p class="mb-2 text-sm">Confirm approval for this task?</p>
            <div class="flex gap-2">
              <Button
                size="sm"
                class="bg-[var(--apollia-success)] text-white hover:bg-[var(--apollia-success)]/90"
                onclick={handleApprove}
                disabled={approving}
                data-testid="approval-confirm-btn"
              >
                {approving ? "Approving..." : "Confirm"}
              </Button>
              <Button size="sm" variant="outline" onclick={cancelDialogs}>
                Cancel
              </Button>
            </div>
          </div>
        {:else if showRejectDialog}
          <div class="rounded border border-[hsl(var(--destructive))] bg-[hsl(var(--destructive))]/5 p-3">
            <p class="mb-2 text-sm">Reason for rejection (min {MIN_REASON_LENGTH} characters):</p>
            <textarea
              class="mb-2 w-full rounded-md border bg-background px-3 py-2 text-sm"
              rows="3"
              placeholder="Describe why you are rejecting this action..."
              bind:value={rejectReason}
            ></textarea>
            <p class="mb-2 text-xs text-muted-foreground">
              {rejectReason.trim().length} / {MIN_REASON_LENGTH} min
            </p>
            <div class="flex gap-2">
              <Button
                size="sm"
                variant="destructive"
                onclick={handleReject}
                disabled={!reasonValid || rejecting}
              >
                {rejecting ? "Rejecting..." : "Reject"}
              </Button>
              <Button size="sm" variant="outline" onclick={cancelDialogs}>
                Cancel
              </Button>
            </div>
          </div>
        {:else}
          <div class="flex gap-2">
            <Button
              size="sm"
              class="bg-[var(--apollia-success)] text-white hover:bg-[var(--apollia-success)]/90"
              onclick={openApproveConfirm}
              data-testid="approval-approve-btn"
            >
              Approve
            </Button>
            <Button size="sm" variant="destructive" onclick={openRejectDialog} data-testid="approval-reject-btn">
              Reject
            </Button>
          </div>
        {/if}
      </div>
    </CardContent>
  </Card>
{/if}
