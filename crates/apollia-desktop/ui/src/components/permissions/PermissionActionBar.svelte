<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import { Button } from "$lib/components/ui/button";
  import { Textarea } from "$lib/components/ui/textarea";
  import { addToast } from "$lib/components/ui/toast/store";

  interface Props {
    /** Identifiant de la tâche suspendue à résoudre. */
    taskId: string;
    /** Nom de l'outil pour la règle de préfixe persistée. */
    toolName: string;
    /** Préfixe d'argument pour la règle "Toujours autoriser" (`undefined` = règle globale). */
    argPrefix?: string;
    /** Appelé une fois la décision confirmée par le runtime. */
    onResolved: () => void;
  }

  let { taskId, toolName, argPrefix, onResolved }: Props = $props();

  const MIN_REASON_LENGTH = 10;

  let isProcessing = $state(false);
  let error = $state<string | null>(null);
  let showRejectForm = $state(false);
  let rejectReason = $state("");
  let reasonValid = $derived(rejectReason.trim().length >= MIN_REASON_LENGTH);

  async function handleApprove(): Promise<void> {
    isProcessing = true;
    error = null;
    try {
      await invoke("resume_task", { taskId, approved: true, reason: null });
      onResolved();
    } catch (err: unknown) {
      error = err instanceof Error ? err.message : String(err);
      isProcessing = false;
    }
  }

  async function handleReject(): Promise<void> {
    if (!reasonValid) return;
    isProcessing = true;
    error = null;
    try {
      await invoke("resume_task", {
        taskId,
        approved: false,
        reason: rejectReason.trim(),
      });
      onResolved();
    } catch (err: unknown) {
      error = err instanceof Error ? err.message : String(err);
      isProcessing = false;
    }
  }

  async function handleAlwaysAllow(): Promise<void> {
    isProcessing = true;
    error = null;
    try {
      await invoke("add_permission_prefix_rule", {
        toolName,
        argPrefix: argPrefix ?? null,
        action: "allow",
      });
      await invoke("resume_task", { taskId, approved: true, reason: null });
      addToast($t("permissions.always_allow_success"), "success");
      onResolved();
    } catch (err: unknown) {
      error = err instanceof Error ? err.message : String(err);
      isProcessing = false;
    }
  }

  function openRejectForm(): void {
    showRejectForm = true;
    rejectReason = "";
  }

  function closeRejectForm(): void {
    showRejectForm = false;
    rejectReason = "";
  }
</script>

<div class="mt-3 space-y-2">
  {#if error}
    <p class="text-[11px] text-destructive" data-testid="permission-action-error">{error}</p>
  {/if}

  {#if showRejectForm}
    <div class="space-y-1.5">
      <Textarea
        bind:value={rejectReason}
        rows={2}
        placeholder={$t("permissions.reject_reason_placeholder")}
        aria-label={$t("permissions.reject_reason_placeholder")}
        data-testid="permission-reject-reason"
      />
      <p class="text-[10px] text-muted-foreground">
        {rejectReason.trim().length} / {MIN_REASON_LENGTH}
        {$t("permissions.reject_reason_min", { values: { min: MIN_REASON_LENGTH } })}
      </p>
      <div class="flex gap-2">
        <Button
          size="sm"
          variant="destructive"
          class="h-7 px-3 text-[11px]"
          disabled={!reasonValid || isProcessing}
          onclick={handleReject}
          data-testid="permission-reject-confirm"
        >
          {isProcessing ? $t("permissions.rejecting") : $t("permissions.reject")}
        </Button>
        <Button
          size="sm"
          variant="ghost"
          class="h-7 px-3 text-[11px]"
          disabled={isProcessing}
          onclick={closeRejectForm}
        >
          {$t("common.cancel")}
        </Button>
      </div>
    </div>
  {:else}
    <div class="flex flex-wrap gap-2">
      <Button
        size="sm"
        variant="success"
        class="h-7 px-3 text-[11px]"
        disabled={isProcessing}
        onclick={handleApprove}
        data-testid="permission-approve-btn"
      >
        {isProcessing ? $t("permissions.approving") : $t("permissions.approve")}
      </Button>
      <Button
        size="sm"
        variant="destructive"
        class="h-7 px-3 text-[11px]"
        disabled={isProcessing}
        onclick={openRejectForm}
        data-testid="permission-reject-btn"
      >
        {$t("permissions.reject")}
      </Button>
      <Button
        size="sm"
        variant="ghost"
        class="h-7 px-3 text-[11px] text-primary"
        disabled={isProcessing}
        onclick={handleAlwaysAllow}
        data-testid="permission-always-allow-btn"
      >
        {$t("permissions.always_allow")}
      </Button>
    </div>
  {/if}
</div>
