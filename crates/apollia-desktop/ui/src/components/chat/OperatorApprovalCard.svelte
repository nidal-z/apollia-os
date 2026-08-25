<script lang="ts">
  import type { ToolCallView } from "$lib/types";
  import { resolveToolDisplay } from "$lib/tools/tool-display";
  import { authorizeChatTool } from "$lib/ipc/inbox";
  import { t } from "svelte-i18n";
  import { Shield, ChevronDown } from "lucide-svelte";
  import { slide } from "svelte/transition";
  import { Button } from "$lib/components/ui/button";
  import { currentSession } from "$lib/stores/chat";
  import RejectReasonDialog from "../inbox/RejectReasonDialog.svelte";

  interface Props {
    sessionId: string;
    messageId: string;
    /** Unique id of the tool call, correlating this card with its backend
     *  pending-approval slot. Falls back to the tool name when the surface has
     *  no per-call id (historical/reopened messages). */
    toolCallId?: string;
    toolCall: ToolCallView;
  }

  let { sessionId, messageId, toolCallId, toolCall }: Props = $props();

  const resolvedToolCallId = $derived(toolCallId ?? toolCall.tool_name);

  /** Cf. ApprovalCard : grise « Toujours pour ce projet » hors contexte projet. */
  const hasProject = $derived(
    $currentSession?.id === sessionId && $currentSession?.project_id !== null,
  );

  /** Mappe sur l'enum runtime `AlwaysAcceptScope` (snake_case via serde). */
  type AlwaysScope =
    | "this_session"
    | "this_agent"
    | "this_project"
    | "global";

  const display = $derived(resolveToolDisplay(toolCall));
  const ToolIcon = $derived(display.icon);

  let isProcessing = $state(false);
  let error = $state<string | null>(null);
  let scopeOpen = $state(false);
  let rejectDialogOpen = $state(false);

  // Reset the busy state when this card is reused for a different approval, so
  // consecutive HITL prompts do not stay greyed out from a prior decision.
  $effect(() => {
    void messageId;
    void resolvedToolCallId;
    isProcessing = false;
    error = null;
  });

  async function handleAccept(): Promise<void> {
    isProcessing = true;
    error = null;
    try {
      await authorizeChatTool({
        sessionId,
        messageId,
        toolCallId: resolvedToolCallId,
        toolName: toolCall.tool_name,
        decision: "accept",
      });
    } catch (err: unknown) {
      error = err instanceof Error ? err.message : String(err);
      isProcessing = false;
    }
  }

  async function handleRefuse(reason: string): Promise<void> {
    isProcessing = true;
    error = null;
    try {
      await authorizeChatTool({
        sessionId,
        messageId,
        toolCallId: resolvedToolCallId,
        toolName: toolCall.tool_name,
        decision: "refuse",
        reason,
      });
      rejectDialogOpen = false;
    } catch (err: unknown) {
      error = err instanceof Error ? err.message : String(err);
      isProcessing = false;
    }
  }

  async function handleAlwaysAccept(scope: AlwaysScope): Promise<void> {
    isProcessing = true;
    error = null;
    try {
      await authorizeChatTool({
        sessionId,
        messageId,
        toolCallId: resolvedToolCallId,
        toolName: toolCall.tool_name,
        decision: "always_accept",
        scope,
      });
    } catch (err: unknown) {
      error = err instanceof Error ? err.message : String(err);
      isProcessing = false;
      scopeOpen = false;
    }
  }
</script>

<div
  class="my-1.5 rounded-lg bg-surface-1 border border-border/60 border-l-2 border-l-warning px-3 py-2.5 text-xs"
  data-testid="operator-approval-{toolCall.tool_name}"
  transition:slide={{ duration: 200 }}
>
  <!-- Title row: shield icon + generic action label -->
  <div class="flex items-center gap-2 font-medium text-foreground">
    <div class="flex h-6 w-6 items-center justify-center rounded-lg bg-warning/10">
      <Shield class="h-3.5 w-3.5 text-warning" />
    </div>
    <span class="text-sm">{$t("chat.authorize_action")}</span>
  </div>

  <!-- Human-readable action description with tool icon -->
  <div class="mt-3 flex items-center gap-2">
    <ToolIcon class="h-5 w-5 flex-shrink-0 text-muted-foreground" />
    <span class="text-[12px] text-foreground">
      {$t(display.descriptionKey, { values: display.templateParams })}
    </span>
  </div>

  {#if error}
    <p class="mt-1.5 text-[10px] text-destructive">{error}</p>
  {/if}

  <!-- Decision buttons -->
  <div class="mt-4 flex flex-wrap items-center gap-2">
    <Button
      variant="outline"
      size="sm"
      class="h-7 px-3 text-[11px]"
      disabled={isProcessing}
      onclick={handleAccept}
      data-testid="operator-approval-accept-{toolCall.tool_name}"
    >
      {$t("approval.action.allow_once")}
    </Button>
    <Button
      variant="ghost"
      size="sm"
      class="h-7 px-3 text-[11px] text-destructive hover:bg-destructive/10 hover:text-destructive"
      disabled={isProcessing}
      onclick={() => (rejectDialogOpen = true)}
      data-testid="operator-approval-refuse-{toolCall.tool_name}"
    >
      {$t("approval.action.refuse")}
    </Button>
    <Button
      variant="ghost"
      size="sm"
      class="h-7 px-3 text-[11px] text-primary"
      disabled={isProcessing}
      onclick={() => (scopeOpen = !scopeOpen)}
      data-testid="operator-approval-always-toggle-{toolCall.tool_name}"
      aria-expanded={scopeOpen}
    >
      {$t("approval.action.always_allow")}
      <ChevronDown
        size={11}
        class="ml-0.5 transition-transform {scopeOpen ? 'rotate-180' : ''}"
      />
    </Button>
  </div>

  {#if scopeOpen}
    <div
      class="mt-2 grid grid-cols-1 gap-1.5 rounded-md bg-surface-2/40 p-2 sm:grid-cols-2"
      transition:slide={{ duration: 150 }}
      data-testid="operator-approval-scope-menu-{toolCall.tool_name}"
    >
      <button
        type="button"
        class="rounded-md px-2.5 py-1.5 text-left text-[11px] hover:bg-surface-1 transition-colors disabled:opacity-50"
        disabled={isProcessing}
        onclick={() => handleAlwaysAccept("this_session")}
        data-testid="operator-approval-scope-session-{toolCall.tool_name}"
      >
        <div class="font-medium text-foreground">{$t("approval.scope.session_title")}</div>
        <div class="text-[10px] text-muted-foreground">
          {$t("approval.scope.session_desc")}
        </div>
      </button>
      <button
        type="button"
        class="rounded-md px-2.5 py-1.5 text-left text-[11px] hover:bg-surface-1 transition-colors disabled:opacity-50"
        disabled={isProcessing}
        onclick={() => handleAlwaysAccept("this_agent")}
        data-testid="operator-approval-scope-agent-{toolCall.tool_name}"
      >
        <div class="font-medium text-foreground">{$t("approval.scope.agent_title")}</div>
        <div class="text-[10px] text-muted-foreground">
          {$t("approval.scope.agent_desc_apollia")}
        </div>
      </button>
      <button
        type="button"
        class="rounded-md px-2.5 py-1.5 text-left text-[11px] hover:bg-surface-1 transition-colors disabled:cursor-not-allowed disabled:opacity-50 disabled:hover:bg-transparent"
        disabled={isProcessing || !hasProject}
        onclick={() => handleAlwaysAccept("this_project")}
        data-testid="operator-approval-scope-project-{toolCall.tool_name}"
        title={!hasProject ? $t("approval.scope.project_no_project") : undefined}
      >
        <div class="font-medium text-foreground">{$t("approval.scope.project_title")}</div>
        <div class="text-[10px] text-muted-foreground">
          {hasProject
            ? $t("approval.scope.project_desc")
            : $t("approval.scope.project_unavailable")}
        </div>
      </button>
      <button
        type="button"
        class="rounded-md px-2.5 py-1.5 text-left text-[11px] hover:bg-surface-1 transition-colors disabled:opacity-50"
        disabled={isProcessing}
        onclick={() => handleAlwaysAccept("global")}
        data-testid="operator-approval-scope-global-{toolCall.tool_name}"
      >
        <div class="font-medium text-foreground">{$t("approval.scope.global_title")}</div>
        <div class="text-[10px] text-warning">
          {$t("approval.scope.global_desc")}
        </div>
      </button>
    </div>
  {/if}
</div>

<RejectReasonDialog
  open={rejectDialogOpen}
  submitting={isProcessing}
  onclose={() => (rejectDialogOpen = false)}
  onconfirm={handleRefuse}
/>
