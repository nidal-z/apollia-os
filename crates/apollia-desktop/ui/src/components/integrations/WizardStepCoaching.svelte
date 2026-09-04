<script lang="ts">
  import { t } from "svelte-i18n";
  import { onMount } from "svelte";
  import { AlertTriangle } from "lucide-svelte";
  import ApprovalLevelSelector from "$lib/components/operator/approval/ApprovalLevelSelector.svelte";
  import CapabilityExampleCard from "./CapabilityExampleCard.svelte";
  import { Spinner } from "$lib/components/ui/progress";
  import {
    metaGenerateCapabilitiesCoaching,
    type CoachingExample,
  } from "$lib/ipc/connections";

  type ApprovalLevel = "auto" | "ask";

  interface Props {
    serverName: string;
    serverTitle: string | null;
    hasWriteTools: boolean;
    approvalLevel: ApprovalLevel;
    onchange: (level: ApprovalLevel) => void;
    ontry?: (prompt: string) => void;
  }

  let {
    serverName,
    serverTitle,
    hasWriteTools,
    approvalLevel,
    onchange,
    ontry = () => {},
  }: Props = $props();

  let examples = $state<CoachingExample[]>([]);
  let loading = $state(true);
  let loadError = $state<string | null>(null);

  const showDangerousComboWarning = $derived(
    approvalLevel === "auto" && hasWriteTools,
  );

  onMount(() => {
    void loadExamples();
  });

  async function loadExamples(): Promise<void> {
    loading = true;
    loadError = null;
    try {
      examples = await metaGenerateCapabilitiesCoaching(serverName, serverTitle);
    } catch (err: unknown) {
      loadError = err instanceof Error ? err.message : String(err);
      examples = [];
    } finally {
      loading = false;
    }
  }
</script>

<div class="space-y-6" data-testid="wizard-step-coaching">
  <!-- Approval level -->
  <section aria-labelledby="coaching-approval-heading" class="space-y-2">
    <div>
      <h3
        id="coaching-approval-heading"
        class="text-sm font-semibold text-foreground"
      >
        {$t("integrations.wizard.coaching.approval_title")}
      </h3>
      <p class="mt-0.5 text-xs text-muted-foreground">
        {$t("integrations.wizard.coaching.approval_subtitle")}
      </p>
    </div>
    <ApprovalLevelSelector level={approvalLevel} {onchange} />

    {#if showDangerousComboWarning}
      <div
        class="flex items-start gap-2 rounded-md border border-destructive/30 bg-destructive/5 px-3 py-2"
        data-testid="coaching-dangerous-combo"
      >
        <AlertTriangle
          size={14}
          class="mt-0.5 shrink-0 text-destructive"
          aria-hidden="true"
        />
        <p class="text-xs text-destructive">
          {$t("integrations.wizard.coaching.auto_write_warning")}
        </p>
      </div>
    {/if}
  </section>

  <!-- Post-install coaching -->
  <section aria-labelledby="coaching-examples-heading" class="space-y-3">
    <div>
      <h3
        id="coaching-examples-heading"
        class="text-sm font-semibold text-foreground"
      >
        {$t("integrations.wizard.coaching.examples_title")}
      </h3>
      <p class="mt-0.5 text-xs text-muted-foreground">
        {$t("integrations.wizard.coaching.examples_subtitle")}
      </p>
    </div>

    {#if loading}
      <div
        class="flex items-center gap-2 text-xs text-muted-foreground"
        data-testid="coaching-loading"
      >
        <Spinner size={12} />
        {$t("integrations.wizard.coaching.loading")}
      </div>
    {:else if loadError}
      <p class="text-xs text-muted-foreground" data-testid="coaching-error">
        {$t("integrations.wizard.coaching.error")}
      </p>
    {:else if examples.length === 0}
      <p class="text-xs text-muted-foreground" data-testid="coaching-empty">
        {$t("integrations.wizard.coaching.empty")}
      </p>
    {:else}
      <div class="space-y-2" data-testid="coaching-examples">
        {#each examples as example, i (i)}
          <CapabilityExampleCard
            title={example.title}
            description={example.description}
            prompt={example.prompt}
            ontry={(p) => ontry(p)}
          />
        {/each}
      </div>
    {/if}
  </section>
</div>
