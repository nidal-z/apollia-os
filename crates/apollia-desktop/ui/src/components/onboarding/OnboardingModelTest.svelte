<script lang="ts">
  /**
   * The acquaintance step of the first launch: the model chosen two steps
   * earlier answers one question, so the person sees it work, then the
   * profile is filled by hand in the form of Settings > Profile. Dictation is
   * set up and tried one step earlier, beside the search model.
   *
   * Nothing here is required. The test runs once the model is there, in a
   * conversation of its own that is deleted as soon as the answer is read; the
   * profile is saved or passed over, and both end the step.
   */
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import { AlertCircle, Check, Loader2, RotateCw } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { createChatSession } from "$lib/ipc/agents";
  import { deleteChatSession, getChatSession, sendChatMessage } from "$lib/ipc/chat";
  import { finalizeOnboardingChat } from "$lib/ipc/onboarding";
  import { deriveLlmState } from "$lib/onboarding/llmState";
  import { llmBackends, llmBackendsHydrated, refreshLlmBackends } from "$lib/stores/sse";
  import { isInFlight } from "./llmDownload";
  import { llmDownload } from "./llmDownloadLive";
  import { MODEL_TEST_POLL_MS, modelTestOutcome } from "./modelTest";
  import OnboardingProfileReview from "./OnboardingProfileReview.svelte";

  interface Props {
    onback: () => void;
    /** Called once the profile was saved or passed over. */
    onclose: () => void;
  }

  const { onback, onclose }: Props = $props();

  type Test =
    | { phase: "idle" }
    | { phase: "running" }
    | { phase: "answered"; answer: string; seconds: number }
    | { phase: "failed"; reason: string | null };

  let test = $state<Test>({ phase: "idle" });
  let started = false;
  let alive = true;

  const llmState = $derived(
    deriveLlmState($llmBackendsHydrated, $llmBackends.length, isInFlight($llmDownload)),
  );

  const wait = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms));

  async function runTest(): Promise<void> {
    test = { phase: "running" };
    const began = Date.now();
    let sessionId: string | null = null;
    try {
      const session = await createChatSession({ mode: "libre" });
      sessionId = session.id;
      await sendChatMessage(sessionId, $t("onboarding_model_test.prompt"));
      while (alive) {
        await wait(MODEL_TEST_POLL_MS);
        const outcome = modelTestOutcome(await getChatSession(sessionId), Date.now() - began);
        if (outcome.kind === "answered") {
          test = {
            phase: "answered",
            answer: outcome.answer,
            seconds: Math.max(1, Math.round((Date.now() - began) / 1000)),
          };
          break;
        }
        if (outcome.kind === "timed_out") {
          test = { phase: "failed", reason: null };
          break;
        }
      }
    } catch (err) {
      test = { phase: "failed", reason: err instanceof Error ? err.message : String(err) };
    } finally {
      // The conversation only carried the test: it leaves no trace in the chat.
      if (sessionId) void deleteChatSession(sessionId).catch(() => undefined);
    }
  }

  // The test starts by itself the moment the model is there, once.
  $effect(() => {
    if (llmState === "ready" && !started) {
      started = true;
      void runTest();
    }
  });

  onMount(() => {
    void refreshLlmBackends();
    // The step holds no conversation to finish: the gate the profile form
    // reads is stamped at once, as the direct skip of the questions did.
    void finalizeOnboardingChat().catch(() => undefined);
    return () => {
      alive = false;
    };
  });
</script>

<div class="flex min-h-0 flex-1 flex-col" data-testid="onboarding-model-test-step">
  <section
    class="mx-6 mt-5 shrink-0 rounded-xl border px-4 py-3 {test.phase === 'failed'
      ? 'border-warning/40 bg-warning/5'
      : test.phase === 'answered'
        ? 'border-success/40 bg-success/5'
        : 'border-border bg-surface-1'}"
    aria-live="polite"
    data-testid="onboarding-model-test"
    data-phase={test.phase}
  >
    <div class="flex items-center gap-2">
      {#if test.phase === "answered"}
        <Check size={15} class="shrink-0 text-success" aria-hidden="true" />
      {:else if test.phase === "failed"}
        <AlertCircle size={15} class="shrink-0 text-warning-a11y" aria-hidden="true" />
      {:else}
        <Loader2 size={15} class="shrink-0 animate-spin text-muted-foreground" aria-hidden="true" />
      {/if}
      <p class="min-w-0 flex-1 text-body-sm font-medium text-foreground">
        {#if test.phase === "answered"}
          {$t("onboarding_model_test.answered", { values: { seconds: test.seconds } })}
        {:else if test.phase === "failed"}
          {$t("onboarding_model_test.failed")}
        {:else if llmState === "downloading"}
          {$t("onboarding_model_test.downloading")}
        {:else if llmState === "none"}
          {$t("onboarding_model_test.no_model")}
        {:else}
          {$t("onboarding_model_test.running")}
        {/if}
      </p>
      {#if test.phase === "answered" || test.phase === "failed"}
        <Button variant="ghost" size="sm" onclick={() => void runTest()} data-testid="onboarding-model-test-retry">
          {#snippet icon()}<RotateCw size={13} />{/snippet}
          {$t("onboarding_model_test.retry")}
        </Button>
      {/if}
    </div>
    {#if test.phase === "answered"}
      <p class="mt-2 text-caption text-muted-foreground">{$t("onboarding_model_test.prompt")}</p>
      <p class="mt-1 line-clamp-4 text-body-sm text-foreground" data-testid="onboarding-model-test-answer">
        {test.answer}
      </p>
    {:else if test.phase === "failed"}
      <p class="mt-1 text-caption text-muted-foreground">
        {test.reason ?? $t("onboarding_model_test.timed_out")}
        {$t("onboarding_model_test.failed_hint")}
      </p>
    {/if}
  </section>

  <OnboardingProfileReview proposal={null} ondone={onclose} {onback} />
</div>
