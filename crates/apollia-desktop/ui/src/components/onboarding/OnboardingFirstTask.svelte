<script lang="ts">
  /**
   * OnboardingFirstTask - what the first launch closes on, once its modal is
   * gone: a conversation with the agent kept, its composer holding the first
   * example the agent declares; for an example that comes with a
   * demonstration dataset, the copy of that dataset then the launch of a
   * batch on the copied workbook; without an agent, a conversation with
   * Apollia.
   *
   * Mounted above every screen, since the modal it follows is. The decisions
   * live in `firstTask.ts`; this component holds the two dialogs and the
   * calls. Leaving the copy, or the launch, falls back on the conversation
   * with the agent: nothing is copied or launched unasked.
   */
  import { tick } from "svelte";
  import { reportError } from "$lib/errors/reportError";
  import { getAgentDefinition } from "$lib/ipc/agentSheet";
  import { createChatSession } from "$lib/ipc/agents";
  import { requestChatInputAppend } from "$lib/stores/artifacts";
  import { openBatch } from "$lib/stores/batchView";
  import { pendingChatSessionId } from "$lib/stores/chat";
  import { navigateTo } from "$lib/stores/navigation";
  import ExampleTrialDialog from "../agents/ExampleTrialDialog.svelte";
  import type { Trial } from "../agents/exampleTrial";
  import BatchLaunchDialog from "../batch/BatchLaunchDialog.svelte";
  import { openFirstConversation, type FirstConversation } from "./firstTask";
  import { firstTaskIntro, firstTaskRequest } from "./firstTaskLive";

  let trial = $state<Trial | null>(null);
  let trialBatch = $state<{ agent: string; workbook: string } | null>(null);
  // The conversation opened when the copy, or the launch, is left.
  let fallback: FirstConversation | null = null;

  async function converse(conversation: FirstConversation): Promise<void> {
    await openFirstConversation(
      {
        createSession: createChatSession,
        agentExamples: async (agentId) => (await getAgentDefinition(agentId))?.examples ?? null,
        rememberSession: (sessionId) => pendingChatSessionId.set(sessionId),
        // The chat screen takes the session before its composer is written in.
        navigate: async () => {
          navigateTo("chat");
          await tick();
        },
        prefill: requestChatInputAppend,
        introduce: (intro) => firstTaskIntro.set(intro),
        report: (err) => {
          reportError(err, { surface: "toast", testid: "first-task-error" });
        },
      },
      conversation,
    );
  }

  $effect(() => {
    const plan = $firstTaskRequest;
    if (plan === null) return;
    firstTaskRequest.set(null);
    if (plan.kind === "trial") {
      fallback = plan.fallback;
      trial = plan.trial;
    } else {
      void converse(plan);
    }
  });

  function leave(): void {
    const conversation = fallback;
    trial = null;
    trialBatch = null;
    fallback = null;
    if (conversation !== null) void converse(conversation);
  }

  function launchTrialBatch(agent: string, workbook: string): void {
    trial = null;
    trialBatch = { agent, workbook };
  }
</script>

<ExampleTrialDialog {trial} onclose={leave} onLaunchBatch={launchTrialBatch} />

<BatchLaunchDialog
  open={trialBatch !== null}
  agent={trialBatch?.agent ?? null}
  file={trialBatch?.workbook ?? null}
  onclose={leave}
  onlaunched={(batch) => {
    // Launched: the view of the batch is what the first launch closes on.
    fallback = null;
    openBatch(batch.batchId, "launch");
  }}
/>
