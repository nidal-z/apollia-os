<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { Button } from "$lib/components/ui/button";

  interface Props {
    onContinue: () => void;
    onSkip: () => void;
  }

  let { onContinue, onSkip }: Props = $props();

  interface CheckResult {
    label: string;
    passed: boolean | null;
    helpText?: string;
  }

  let checks = $state<CheckResult[]>([
    { label: "Runtime started", passed: null },
    { label: "Python 3 detected", passed: null },
    { label: "LLM backend configured", passed: null },
  ]);

  let allChecked = $derived(checks.every((c) => c.passed !== null));
  let pythonOk = $derived(checks[1].passed === true);
  let canContinue = $derived(allChecked && pythonOk);

  async function runChecks() {
    checks[0] = { ...checks[0], passed: true };

    const [pythonResult, llmResult] = await Promise.allSettled([
      invoke<boolean>("check_python"),
      invoke<boolean>("check_llm_configured"),
    ]);

    const hasPython =
      pythonResult.status === "fulfilled" && pythonResult.value === true;
    checks[1] = {
      ...checks[1],
      passed: hasPython,
      helpText: hasPython
        ? undefined
        : "Python 3 is required to run agents. Install it from python.org or via your package manager.",
    };

    const hasLlm =
      llmResult.status === "fulfilled" && llmResult.value === true;
    checks[2] = {
      ...checks[2],
      passed: hasLlm,
      helpText: hasLlm
        ? undefined
        : "No LLM backend configured. Add one in apollia.toml to enable AI capabilities.",
    };
  }

  onMount(() => {
    runChecks();
  });
</script>

<div class="space-y-6">
  <div>
    <h2 class="text-xl font-semibold">Environment check</h2>
    <p class="mt-1 text-sm text-muted-foreground">
      Verifying that your system is ready to run Apollia OS agents.
    </p>
  </div>

  <div class="space-y-3">
    {#each checks as check}
      <div class="flex items-start gap-3 rounded-lg border bg-card p-4">
        <span class="mt-0.5 text-lg">
          {#if check.passed === null}
            <span
              class="inline-block h-5 w-5 animate-spin rounded-full border-2 border-muted-foreground border-t-transparent"
            ></span>
          {:else if check.passed}
            <span class="text-[var(--apollia-success)]">&#10003;</span>
          {:else}
            <span class="text-[hsl(var(--destructive))]">&#10007;</span>
          {/if}
        </span>
        <div class="flex-1">
          <p class="text-sm font-medium">{check.label}</p>
          {#if check.helpText}
            <p class="mt-1 text-xs text-[hsl(var(--destructive))]">
              {check.helpText}
            </p>
          {/if}
        </div>
      </div>
    {/each}
  </div>

  <div class="flex justify-end gap-3">
    <Button variant="ghost" onclick={onSkip}>Skip</Button>
    <Button disabled={!canContinue} onclick={onContinue}>Continue</Button>
  </div>
</div>
