<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import { Button } from "$lib/components/ui/button";
  import type { UIMode } from "$lib/stores/mode";

  interface Props {
    mode: UIMode;
    onContinue: () => void;
    onSkip: () => void;
  }

  let { mode, onContinue, onSkip }: Props = $props();

  let isOperator = $derived(mode === "operator");

  interface CheckResult {
    labelKey: string;
    passed: boolean | null;
    helpTextKey?: string;
  }

  let checks = $state<CheckResult[]>([
    { labelKey: "onboarding.check_runtime", passed: null },
    { labelKey: "onboarding.check_python", passed: null },
    { labelKey: "onboarding.check_llm", passed: null },
  ]);

  let displayChecks = $derived(
    checks.map((c) => {
      if (!isOperator) return c;
      const operatorLabels: Record<string, string> = {
        "onboarding.check_runtime": "onboarding.check_runtime_operator",
        "onboarding.check_python": "onboarding.check_python_operator",
        "onboarding.check_llm": "onboarding.check_llm_operator",
      };
      const operatorHelp: Record<string, string> = {
        "onboarding.python_help": "onboarding.python_help_operator",
        "onboarding.llm_help": "onboarding.llm_help_operator",
      };
      return {
        ...c,
        labelKey: operatorLabels[c.labelKey] ?? c.labelKey,
        helpTextKey: c.helpTextKey
          ? (operatorHelp[c.helpTextKey] ?? c.helpTextKey)
          : undefined,
      };
    })
  );

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
      helpTextKey: hasPython ? undefined : "onboarding.python_help",
    };

    const hasLlm =
      llmResult.status === "fulfilled" && llmResult.value === true;
    checks[2] = {
      ...checks[2],
      passed: hasLlm,
      helpTextKey: hasLlm ? undefined : "onboarding.llm_help",
    };
  }

  onMount(() => {
    runChecks();
  });
</script>

<div class="space-y-6" data-testid="step-environment">
  <div>
    <h2 class="text-xl font-medium">
      {$t(isOperator ? 'onboarding.env_check_title_operator' : 'onboarding.env_check_title')}
    </h2>
    <p class="mt-1 text-sm text-muted-foreground">
      {$t(isOperator ? 'onboarding.env_check_subtitle_operator' : 'onboarding.env_check_subtitle')}
    </p>
  </div>

  <div class="space-y-3">
    {#each displayChecks as check}
      <div class="flex items-start gap-3 rounded-lg glass-border glass-card p-4">
        <span class="mt-0.5 text-lg">
          {#if check.passed === null}
            <span
              class="inline-block h-5 w-5 animate-spin rounded-full border-2 border-muted-foreground border-t-transparent"
            ></span>
          {:else if check.passed}
            <span class="text-success">&#10003;</span>
          {:else}
            <span class="text-destructive">&#10007;</span>
          {/if}
        </span>
        <div class="flex-1">
          <p class="text-sm font-medium">{$t(check.labelKey)}</p>
          {#if check.helpTextKey}
            <p class="mt-1 text-xs text-destructive">
              {$t(check.helpTextKey)}
            </p>
          {/if}
        </div>
      </div>
    {/each}
  </div>

  <div class="flex justify-end gap-3">
    <Button variant="ghost" onclick={onSkip}>{$t('common.skip')}</Button>
    <Button disabled={!canContinue} onclick={onContinue}>{$t('common.continue')}</Button>
  </div>
</div>
