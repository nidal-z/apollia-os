<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onDestroy } from "svelte";
  import { t } from "svelte-i18n";
  import type { PendingApproval, ApollaPermission } from "$lib/types";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { Dialog } from "$lib/components/ui/dialog";
  import { Textarea } from "$lib/components/ui/textarea";
  import { addToast } from "$lib/components/ui/toast/store";
  import PermissionDispatcher from "../permissions/PermissionDispatcher.svelte";

  interface Props {
    approval: PendingApproval;
  }

  let { approval }: Props = $props();

  /**
   * Si le contexte porte un `permission_type` reconnu, retourne la demande
   * de permission typée. Sinon retourne `null` (affichage générique).
   */
  function extractPermission(
    ctx: Record<string, unknown> | undefined,
  ): ApollaPermission | null {
    if (ctx === undefined || typeof ctx.permission_type !== "string") return null;
    return ctx as unknown as ApollaPermission;
  }

  const permission = $derived(extractPermission(approval.context));

  const WARNING_THRESHOLD_MS = 1_800_000;
  const MIN_REASON_LENGTH = 10;

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

  let showApproveDialog = $state(false);
  let approving = $state(false);

  let showRejectDialog = $state(false);
  let rejectReason = $state("");
  let rejecting = $state(false);
  let reasonValid = $derived(rejectReason.trim().length >= MIN_REASON_LENGTH);

  let resolved = $state(false);

  async function handleApprove() {
    approving = true;
    try {
      await invoke("resume_task", {
        taskId: approval.task_id,
        approved: true,
        reason: null,
      });
      resolved = true;
      showApproveDialog = false;
      addToast($t("approvals.approved_toast"), "success");
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      addToast(msg, "error");
    } finally {
      approving = false;
    }
  }

  async function handleReject() {
    if (!reasonValid) return;
    rejecting = true;
    try {
      await invoke("resume_task", {
        taskId: approval.task_id,
        approved: false,
        reason: rejectReason.trim(),
      });
      resolved = true;
      showRejectDialog = false;
      addToast($t("approvals.rejected_toast"), "success");
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      addToast(msg, "error");
    } finally {
      rejecting = false;
    }
  }

  function openApproveDialog() {
    showApproveDialog = true;
  }

  function openRejectDialog() {
    showRejectDialog = true;
    rejectReason = "";
  }

  function shortId(id: string): string {
    return id.slice(0, 8);
  }
</script>

{#if !resolved}
  <div class="glass-card-hover relative overflow-hidden" data-testid="approval-card" data-task-id={approval.task_id}>
    <!-- Status accent bar -->
    <div
      class="h-0.5 w-full {isOverThreshold ? 'bg-destructive' : 'bg-warning'}"
      data-testid="approval-status-bar"
    ></div>

    <div class="px-3.5 pt-3 pb-2.5">
      <!-- Header -->
      <div class="flex items-center justify-between">
        <div class="flex items-center gap-2">
          <h3 class="text-[13px] font-medium">
            {approval.agent_name || $t("approvals.unknown_agent")}
          </h3>
          <code class="text-[11px] text-muted-foreground">{shortId(approval.task_id)}</code>
        </div>
        <Badge variant={isOverThreshold ? "destructive" : "warning"}>
          {formatElapsed(elapsedMs)}
        </Badge>
      </div>

      {#if permission}
        <!-- Typed permission view: badge + preview + 3 buttons -->
        <div class="mt-2">
          <PermissionDispatcher
            taskId={approval.task_id}
            {permission}
            onResolved={() => { resolved = true; }}
          />
        </div>
      {:else}
        <!-- Generic prompt + context JSON + approve/reject -->
        <div class="mt-2">
          <p class="text-[11px] text-muted-foreground mb-1">{$t("approvals.prompt_label")}</p>
          <div class="max-h-[400px] overflow-auto rounded glass-border glass-inset p-2.5">
            <p class="whitespace-pre-wrap text-[13px]">{approval.prompt || $t("approvals.no_prompt")}</p>
          </div>
        </div>

        {#if approval.context}
          <details class="mt-2 glass-border glass-inset rounded-md" data-testid="approval-context-details">
            <summary class="text-[11px] text-muted-foreground px-2 py-1 cursor-pointer">
              {$t("approvals.show_context")}
            </summary>
            <pre class="text-[11px] px-2 py-1 overflow-x-auto">{JSON.stringify(approval.context, null, 2)}</pre>
          </details>
        {/if}

        <div class="flex gap-2 mt-3">
          <Button
            size="sm"
            variant="success"
            onclick={openApproveDialog}
            data-testid="approval-approve-btn"
          >
            {$t("approvals.approve")}
          </Button>
          <Button
            size="sm"
            variant="destructive"
            onclick={openRejectDialog}
            data-testid="approval-reject-btn"
          >
            {$t("approvals.reject")}
          </Button>
        </div>
      {/if}
    </div>
  </div>

  <!-- Approve Dialog -->
  <Dialog
    open={showApproveDialog}
    onclose={() => (showApproveDialog = false)}
    title={$t("approvals.confirm_approval")}
    size="sm"
    data-testid="approval-approve-dialog"
  >
    <p class="text-sm text-muted-foreground">
      {$t("approvals.confirm_approval_message", { values: { agent: approval.agent_name, taskId: shortId(approval.task_id) } })}
    </p>
    <div class="mt-6 flex justify-end gap-2">
      <Button variant="outline" onclick={() => (showApproveDialog = false)}>
        {$t("common.cancel")}
      </Button>
      <Button
        variant="success"
        onclick={handleApprove}
        disabled={approving}
        data-testid="approval-confirm-btn"
      >
        {approving ? $t("approvals.approving") : $t("approvals.approve")}
      </Button>
    </div>
  </Dialog>

  <!-- Reject Dialog -->
  <Dialog
    open={showRejectDialog}
    onclose={() => (showRejectDialog = false)}
    title={$t("approvals.reject")}
    size="sm"
    data-testid="approval-reject-dialog"
  >
    <p class="mb-3 text-sm text-muted-foreground">
      {$t("approvals.reject_reason", { values: { min: MIN_REASON_LENGTH } })}
    </p>
    <Textarea
      bind:value={rejectReason}
      rows={3}
      placeholder={$t("approvals.reject_placeholder")}
      aria-label={$t("approvals.reject_placeholder")}
      data-testid="approval-reject-reason"
    />
    <p class="mt-1 text-[11px] text-muted-foreground">
      {rejectReason.trim().length} / {MIN_REASON_LENGTH} min
    </p>
    <div class="mt-4 flex justify-end gap-2">
      <Button variant="outline" onclick={() => (showRejectDialog = false)}>
        {$t("common.cancel")}
      </Button>
      <Button
        variant="destructive"
        onclick={handleReject}
        disabled={!reasonValid || rejecting}
      >
        {rejecting ? $t("approvals.rejecting") : $t("approvals.reject")}
      </Button>
    </div>
  </Dialog>
{/if}
