<script lang="ts">
  import { t } from "svelte-i18n";
  import { slide } from "svelte/transition";
  import { ShieldAlert, Zap } from "lucide-svelte";
  import { authorizeChatTool } from "$lib/ipc/inbox";
  import { Button } from "$lib/components/ui/button";
  import { developerMode } from "$lib/stores/developerMode";
  import ConsentAnswers, { type ConsentDecision } from "./ConsentAnswers.svelte";
  import CodeToRun from "./CodeToRun.svelte";
  import { isCodeExecutor, isSingleSimpleCommand, offeredAnswers } from "./codeConsent";

  interface Props {
    sessionId: string;
    messageId: string;
    toolCallId: string;
    toolName: string;
    /** Short preview of the call, or the whole code of a tool that runs code. */
    inputPreview: string;
    /** The answers the request offers; derived from the tool when absent. */
    answers?: readonly string[] | null;
    /** The full arguments of the call, shown behind "see more". */
    detail?: string;
    /** The whole command or source of a tool that runs code. */
    code?: string | null;
    /** The folder the code runs in. */
    workingDir?: string | null;
  }

  let {
    sessionId,
    messageId,
    toolCallId,
    toolName,
    inputPreview,
    answers = null,
    detail = "",
    code = null,
    workingDir = null,
  }: Props = $props();

  const runsCode = $derived(code !== null || isCodeExecutor(toolName));
  const codeText = $derived(code ?? inputPreview);
  const offered = $derived(offeredAnswers(toolName, answers));
  const compound = $derived(
    runsCode && $developerMode && !offered.includes("always") && !isSingleSimpleCommand(codeText),
  );
  const hasDetail = $derived(!runsCode && detail.trim().length > 0 && detail !== inputPreview);
  let showDetail = $state(false);

  let isProcessing = $state(false);
  let error = $state<string | null>(null);

  // Consecutive HITL prompts reuse this same card instance (the parent holds a
  // single approval slot). isProcessing is only cleared on error, so without
  // this the buttons stay disabled (greyed out) from the previous approval,
  // forcing the user to the inbox. Reset the busy state whenever the card is
  // pointed at a different approval (its message/tool identity changes).
  $effect(() => {
    void messageId;
    void toolCallId;
    isProcessing = false;
    error = null;
    showDetail = false;
  });

  /** True when this is an A2A worker-agent delegation. */
  const isA2A = $derived(toolName.startsWith("a2a:"));
  /** Skill ID (e.g. "read-excel") extracted from "a2a:read-excel". */
  const a2aSkillId = $derived(isA2A ? toolName.slice(4) : null);

  /** Sends one of the three answers. A refusal acts at once, with the reason
   *  typed beside it if there is one, which the agent reads in its result. */
  async function handleAnswer({ decision, reason }: ConsentDecision): Promise<void> {
    isProcessing = true;
    error = null;
    try {
      await authorizeChatTool({
        sessionId,
        messageId,
        toolCallId,
        toolName,
        decision,
        reason,
      });
    } catch (err: unknown) {
      error = err instanceof Error ? err.message : String(err);
      isProcessing = false;
    }
  }
</script>

<div class="bg-surface-1 border border-border/60 my-1.5 rounded-lg border-l-2 px-3 py-2.5 text-xs {isA2A ? 'border-l-secondary' : 'border-l-warning'}" data-testid="approval-card-{toolName}" transition:slide={{ duration: 200 }}>
  <div class="flex items-center gap-2 font-medium text-foreground">
    <div
      class="flex h-6 w-6 items-center justify-center rounded-lg {isA2A
        ? 'bg-secondary/10'
        : 'bg-warning/10'}"
    >
      {#if isA2A}
        <Zap class="h-3.5 w-3.5 text-secondary" />
      {:else}
        <ShieldAlert class="h-3.5 w-3.5 text-warning" />
      {/if}
    </div>
    {#if isA2A}
      <span>
        {$t("chat.a2a_authorize_skill")}
        <strong>{a2aSkillId}</strong>
        {$t("chat.a2a_via_worker")} ?
      </span>
    {:else}
      <span>{$t("chat.authorize_tool")} <strong>{toolName}</strong> ?</span>
    {/if}
  </div>

  {#if runsCode}
    <CodeToRun testidPrefix="approval" {toolName} code={codeText} {workingDir} />
  {:else}
    <pre class="mt-2 whitespace-pre-wrap break-all rounded-lg bg-surface-2 p-2 font-mono text-micro text-muted-foreground" data-testid="approval-preview-{toolName}">{inputPreview}</pre>
    {#if hasDetail}
      {#if showDetail}
        <pre class="mt-2 max-h-72 overflow-auto whitespace-pre-wrap break-all rounded-lg bg-surface-2 p-2 font-mono text-micro text-foreground" data-testid="approval-detail-{toolName}">{detail}</pre>
      {/if}
      <Button
        variant="link"
        size="sm"
        class="mt-1 h-auto px-0 text-caption"
        onclick={() => (showDetail = !showDetail)}
        aria-expanded={showDetail}
        data-testid="approval-see-more-{toolName}"
      >
        {showDetail ? $t("approval.see_less") : $t("approval.see_more")}
      </Button>
    {/if}
  {/if}

  {#if error}
    <p class="mt-1.5 text-micro text-destructive">{error}</p>
  {/if}

  <ConsentAnswers
    testidPrefix="approval"
    {toolName}
    answers={offered}
    {runsCode}
    {compound}
    disabled={isProcessing}
    onanswer={handleAnswer}
  />
</div>
