<!--
  The trial of the search model, in the first launch: the model just installed
  is loaded and asked for the vector of one sentence, so the person sees it
  answer before anything depends on it. Never required.
-->
<script lang="ts">
  import { t } from "svelte-i18n";
  import { AlertCircle, Check, Play } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { testEmbeddingsModel } from "$lib/ipc/embeddings";

  type Trial =
    | { phase: "idle" }
    | { phase: "running" }
    | { phase: "answered"; dimensions: number; seconds: string }
    | { phase: "failed"; reason: string };

  let trial = $state<Trial>({ phase: "idle" });

  async function run(): Promise<void> {
    if (trial.phase === "running") return;
    trial = { phase: "running" };
    try {
      const answer = await testEmbeddingsModel();
      trial = {
        phase: "answered",
        dimensions: answer.dimensions,
        seconds: (answer.elapsed_ms / 1000).toFixed(1),
      };
    } catch (err) {
      trial = { phase: "failed", reason: err instanceof Error ? err.message : String(err) };
    }
  }
</script>

<div class="flex flex-wrap items-center gap-3" data-testid="embeddings-test" data-phase={trial.phase}>
  <Button
    variant="outline"
    size="sm"
    onclick={() => void run()}
    disabled={trial.phase === "running"}
    loading={trial.phase === "running"}
    data-testid="embeddings-test-run"
  >
    {#snippet icon()}<Play size={12} />{/snippet}
    {trial.phase === "running" ? $t("onboarding.embeddings.test_running") : $t("onboarding.embeddings.test")}
  </Button>
  {#if trial.phase === "answered"}
    <span class="flex items-center gap-1.5 text-caption text-success" data-testid="embeddings-test-answered">
      <Check size={13} aria-hidden="true" />
      {$t("onboarding.embeddings.test_answered", {
        values: { seconds: trial.seconds, dimensions: trial.dimensions },
      })}
    </span>
  {:else if trial.phase === "failed"}
    <span class="flex items-center gap-1.5 text-caption text-destructive" role="alert" data-testid="embeddings-test-failed">
      <AlertCircle size={13} aria-hidden="true" />
      {$t("onboarding.embeddings.test_failed")} {trial.reason}
    </span>
  {/if}
</div>
