<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import { Button } from "$lib/components/ui/button";
  import { tasks } from "$lib/stores/sse";
  import { Check } from "lucide-svelte";
  import type { TaskSummary, PendingApproval } from "$lib/types";

  interface Props {
    agentId: string;
    agentName: string;
    onFinish: () => void;
  }

  let { agentId, agentName, onFinish }: Props = $props();

  let inputText = $state("");
  let submitting = $state(false);
  let taskId = $state<string | null>(null);
  let error = $state<string | null>(null);
  let outputText = $state<string | null>(null);

  let pendingApproval = $state<PendingApproval | null>(null);
  let resuming = $state(false);

  let pollTimer: ReturnType<typeof setInterval> | null = null;

  let taskStatus = $derived.by(() => {
    if (!taskId) return null;
    return $tasks.find((t: TaskSummary) => t.id === taskId) ?? null;
  });

  let isCompleted = $derived(taskStatus?.status === "completed");
  let isFailed = $derived(taskStatus?.status === "failed");
  let isInputRequired = $derived(taskStatus?.status === "input_required");
  let isTerminal = $derived(isCompleted || isFailed);

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
      if (taskStatus?.output_text) {
        outputText = taskStatus.output_text;
      }
      return;
    }

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

  $effect(() => () => stopPolling());

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
      startPolling();
    } catch (err: unknown) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      resuming = false;
    }
  }
</script>

<div class="space-y-6" data-testid="step-see-it-work">
  <div>
    <h2 class="text-xl font-medium">{$t('onboarding.see_it_work_title')}</h2>
    <p class="mt-1 text-sm text-muted-foreground">
      {$t('onboarding.see_it_work_subtitle')}
    </p>
    <p class="mt-1 text-xs text-muted-foreground">
      {$t('onboarding.agent_name_label')}: <span class="font-medium text-foreground">{agentName}</span>
    </p>
  </div>

  {#if !taskId}
    <div class="space-y-3">
      <textarea
        class="w-full rounded-lg border bg-background px-4 py-3 text-sm placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-ring"
        rows={3}
        placeholder={$t('onboarding.task_placeholder_operator')}
        bind:value={inputText}
        disabled={submitting}
      ></textarea>
      <div class="flex justify-end">
        <Button onclick={submitTask} disabled={submitting || !inputText.trim()}>
          {submitting ? $t('onboarding.sending') : $t('onboarding.send')}
        </Button>
      </div>
    </div>
  {/if}

  {#if error}
    <div class="rounded-lg border border-destructive bg-destructive/10 p-4">
      <p class="text-sm text-destructive">{error}</p>
    </div>
  {/if}

  {#if taskId && !isTerminal}
    <div class="flex items-center gap-3 rounded-lg glass-border glass-card p-4">
      {#if isInputRequired && pendingApproval}
        <div class="flex-1 space-y-3">
          <p class="text-sm font-semibold text-warning-foreground">{$t('onboarding.approval_required')}</p>
          <p class="text-sm text-muted-foreground">{pendingApproval.prompt}</p>
          <Button size="sm" onclick={approve} disabled={resuming}>
            {resuming ? $t('approvals.approving') : $t('approvals.approve')}
          </Button>
        </div>
      {:else}
        <span class="inline-block h-5 w-5 animate-spin rounded-full border-2 border-primary border-t-transparent"></span>
        <p class="text-sm text-muted-foreground">{$t('onboarding.running_status_operator')}</p>
      {/if}
    </div>
  {/if}

  {#if isCompleted}
    <div class="space-y-3">
      <div class="flex items-center gap-2 text-sm font-medium text-[var(--apollia-success)]">
        <Check size={16} />
        {$t('onboarding.task_completed_operator')}
      </div>
      {#if outputText}
        <div class="rounded-lg glass-border glass-card p-4">
          <p class="mb-2 text-xs font-semibold uppercase text-muted-foreground">{$t('onboarding.see_it_work_result_title')}</p>
          <p class="whitespace-pre-wrap text-sm">{outputText}</p>
        </div>
      {:else}
        <p class="text-sm text-muted-foreground">{$t('onboarding.see_it_work_no_output')}</p>
      {/if}
    </div>
  {/if}

  {#if isFailed}
    <p class="text-sm font-medium text-destructive">
      {$t('onboarding.task_failed_operator')}
    </p>
  {/if}

  <div class="flex justify-end">
    {#if isTerminal}
      <Button onclick={onFinish}>{$t('onboarding.finish_setup')}</Button>
    {/if}
  </div>
</div>
