<script lang="ts">
  import type { ToolCallView } from "$lib/types";
  import { resolveToolDisplay } from "$lib/tools/tool-display";
  import { authorizeChatTool } from "$lib/ipc/inbox";
  import { t } from "svelte-i18n";
  import { Shield } from "lucide-svelte";
  import { slide } from "svelte/transition";
  import { developerMode } from "$lib/stores/developerMode";
  import ConsentAnswers, { type ConsentDecision } from "./ConsentAnswers.svelte";
  import CodeToRun from "./CodeToRun.svelte";
  import { codeOf, isSingleSimpleCommand, offeredAnswers, workingDirOf } from "./codeConsent";

  interface Props {
    sessionId: string;
    messageId: string;
    /** Unique id of the tool call, correlating this card with its backend
     *  pending-approval slot. Falls back to the tool name when the surface has
     *  no per-call id (historical/reopened messages). */
    toolCallId?: string;
    toolCall: ToolCallView;
    /** The answers a live request offers; derived from the tool when absent. */
    answers?: readonly string[] | null;
    /** The whole code a live request carries; read from the call when absent. */
    code?: string | null;
    /** The folder a live request names; read from the call when absent. */
    workingDir?: string | null;
  }

  let {
    sessionId,
    messageId,
    toolCallId,
    toolCall,
    answers = null,
    code: requestCode = null,
    workingDir: requestWorkingDir = null,
  }: Props = $props();

  const resolvedToolCallId = $derived(toolCallId ?? toolCall.tool_name);

  const display = $derived(resolveToolDisplay(toolCall));
  const ToolIcon = $derived(display.icon);

  /** The whole code of a tool that runs code, `null` for any other. */
  const code = $derived(requestCode ?? codeOf(toolCall.tool_name, toolCall.input));
  const workingDir = $derived(requestWorkingDir ?? workingDirOf(toolCall.input));
  const offered = $derived(offeredAnswers(toolCall.tool_name, answers));
  const compound = $derived(
    code !== null && $developerMode && !offered.includes("always") && !isSingleSimpleCommand(code),
  );

  let isProcessing = $state(false);
  let error = $state<string | null>(null);

  // Reset the busy state when this card is reused for a different approval, so
  // consecutive HITL prompts do not stay greyed out from a prior decision.
  $effect(() => {
    void messageId;
    void resolvedToolCallId;
    isProcessing = false;
    error = null;
  });

  /** Sends one of the three answers. A refusal acts at once, with the reason
   *  typed beside it if there is one, which the agent reads in its result. */
  async function handleAnswer({ decision, reason }: ConsentDecision): Promise<void> {
    isProcessing = true;
    error = null;
    try {
      await authorizeChatTool({
        sessionId,
        messageId,
        toolCallId: resolvedToolCallId,
        toolName: toolCall.tool_name,
        decision,
        reason,
      });
    } catch (err: unknown) {
      error = err instanceof Error ? err.message : String(err);
      isProcessing = false;
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
    <span class="text-body-xs text-foreground">
      {$t(display.descriptionKey, { values: display.templateParams })}
    </span>
  </div>

  {#if code !== null}
    <CodeToRun
      testidPrefix="operator-approval"
      toolName={toolCall.tool_name}
      {code}
      {workingDir}
    />
  {/if}

  {#if error}
    <p class="mt-1.5 text-micro text-destructive">{error}</p>
  {/if}

  <ConsentAnswers
    testidPrefix="operator-approval"
    toolName={toolCall.tool_name}
    answers={offered}
    runsCode={code !== null}
    {compound}
    disabled={isProcessing}
    onanswer={handleAnswer}
  />
</div>
