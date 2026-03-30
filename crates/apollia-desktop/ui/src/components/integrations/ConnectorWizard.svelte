<script lang="ts">
  import { t } from "svelte-i18n";
  import { invoke } from "@tauri-apps/api/core";
  import { Dialog } from "$lib/components/ui/dialog";
  import { Button } from "$lib/components/ui/button";
  import McpDisclaimerDialog, {
    isDisclaimerAccepted,
  } from "./McpDisclaimerDialog.svelte";
  import WizardStepInfo from "./WizardStepInfo.svelte";
  import WizardStepAuth from "./WizardStepAuth.svelte";
  import WizardStepParams from "./WizardStepParams.svelte";
  import WizardStepTest from "./WizardStepTest.svelte";
  import WizardStepConfirm from "./WizardStepConfirm.svelte";
  import type {
    McpServerConfigInput,
    RegistryEnvVarView,
    RegistryServerView,
  } from "$lib/types";

  interface Props {
    server: RegistryServerView;
    open: boolean;
    onclose: () => void;
    oncomplete: () => void;
  }

  let { server, open, onclose, oncomplete }: Props = $props();

  type WizardStep = "info" | "auth" | "params" | "test" | "confirm";

  // Disclaimer gate: show the disclaimer dialog first if never accepted.
  let showDisclaimer = $state(false);
  let disclaimerPassed = $state(false);

  $effect(() => {
    if (open) {
      if (!isDisclaimerAccepted()) {
        showDisclaimer = true;
        disclaimerPassed = false;
      } else {
        disclaimerPassed = true;
      }
    } else {
      showDisclaimer = false;
      disclaimerPassed = false;
    }
  });

  // Per-step form data, reset each time the wizard opens.
  let currentStep = $state(1);
  let envValues = $state<Record<string, string>>({});
  let argValues = $state<Record<string, string>>({});
  let approvalLevel = $state<"auto" | "ask" | "readonly">("ask");
  let finalizing = $state(false);
  let finalizeError = $state<string | null>(null);

  $effect(() => {
    if (open) {
      currentStep = 1;
      envValues = {};
      argValues = {};
      approvalLevel = "ask";
      finalizeError = null;
    }
  });

  // Remote takes precedence over package when both are present (more modern transport).
  const remote = $derived(server.remotes[0] ?? null);
  const pkg = $derived(server.packages?.[0] ?? null);
  const connectionMode = $derived<"remote" | "package" | null>(
    remote ? "remote" : pkg ? "package" : null,
  );

  /// Remote headers mapped to RegistryEnvVarView so WizardStepAuth can render them uniformly.
  const remoteHeadersAsEnvVars = $derived.by((): RegistryEnvVarView[] => {
    if (!remote) return [];
    return remote.headers.map((h) => ({
      name: h.name,
      description: h.description,
      is_required: h.isRequired,
      is_secret: h.isSecret,
    }));
  });

  // Derive the ordered list of visible steps based on the server's metadata.
  const visibleSteps = $derived.by((): WizardStep[] => {
    const steps: WizardStep[] = ["info"];
    if (connectionMode === "remote") {
      if (remote && remote.headers.some((h) => h.isRequired || h.isSecret)) {
        steps.push("auth");
      }
    } else if (pkg?.environment_variables?.length) {
      steps.push("auth");
    }
    if (connectionMode === "package" && pkg?.package_arguments?.length) {
      steps.push("params");
    }
    steps.push("test", "confirm");
    return steps;
  });

  const totalSteps = $derived(visibleSteps.length);
  const currentStepName = $derived(visibleSteps[currentStep - 1] ?? "info");

  /// Whether this server can be connected (has a remote endpoint or an installable package).
  const canInstall = $derived(connectionMode !== null);

  /// Build a config object from current form values.
  /// When `forTest` is true, secret values are inlined (raw) so the test endpoint
  /// can use them without a secret store. When false, secrets use
  /// `${APOLLIA_SECRET:*}` placeholders for persistence.
  function buildConfig(forTest: boolean): McpServerConfigInput | null {
    if (connectionMode === "remote" && remote) {
      const env: Record<string, string> = {};
      for (const header of remote.headers) {
        env[header.name] =
          header.isSecret && !forTest
            ? `\${APOLLIA_SECRET:${header.name}}`
            : (envValues[header.name] ?? "");
      }
      return {
        name: server.name,
        url: remote.url,
        transport: remote.type,
        env,
        requires_approval: approvalLevel === "ask",
        tags: [],
      };
    }
    if (connectionMode === "package" && pkg) {
      const env: Record<string, string> = {};
      for (const envVar of pkg.environment_variables ?? []) {
        env[envVar.name] =
          envVar.is_secret && !forTest
            ? `\${APOLLIA_SECRET:${envVar.name}}`
            : (envValues[envVar.name] ?? "");
      }
      return {
        name: server.name,
        command: pkg.runtime_hint ?? "npx",
        args: [
          pkg.identifier,
          ...(pkg.package_arguments ?? []).map(
            (a, i) => argValues[a.value_hint ?? String(i)] ?? "",
          ),
        ],
        env,
        transport: pkg.transport_type ?? "stdio",
        requires_approval: approvalLevel === "ask",
        tags: [],
      };
    }
    return null;
  }

  // Config with placeholder secrets — used for finalize (persist to mcp.toml).
  const builtConfig = $derived(buildConfig(false));
  // Config with raw secret values — used for the ephemeral connection test.
  const testConfig = $derived(buildConfig(true));

  function goNext(): void {
    if (currentStep < totalSteps) currentStep += 1;
  }

  function goBack(): void {
    if (currentStep > 1) currentStep -= 1;
  }

  function handleClose(): void {
    showDisclaimer = false;
    disclaimerPassed = false;
    onclose();
  }

  function handleDisclaimerAccept(): void {
    showDisclaimer = false;
    disclaimerPassed = true;
  }

  function handleDisclaimerClose(): void {
    showDisclaimer = false;
    if (!disclaimerPassed) {
      onclose();
    }
  }

  function handleEnvChange(key: string, value: string): void {
    envValues = { ...envValues, [key]: value };
  }

  function handleArgChange(key: string, value: string): void {
    argValues = { ...argValues, [key]: value };
  }

  async function finalize(): Promise<void> {
    if (!builtConfig) return;
    finalizing = true;
    finalizeError = null;
    try {
      // Persist secrets in the OS keyring before writing the config.
      if (connectionMode === "remote" && remote) {
        for (const header of remote.headers) {
          const val = envValues[header.name];
          if (header.isSecret && val) {
            await invoke("store_mcp_secret", {
              serverName: server.name,
              envVar: header.name,
              value: val,
            });
          }
        }
      } else if (connectionMode === "package" && pkg) {
        for (const envVar of pkg.environment_variables ?? []) {
          const val = envValues[envVar.name];
          if (envVar.is_secret && val) {
            await invoke("store_mcp_secret", {
              serverName: server.name,
              envVar: envVar.name,
              value: val,
            });
          }
        }
      }
      await invoke("add_mcp_server", { config: builtConfig });
      handleClose();
      oncomplete();
    } catch (err: unknown) {
      finalizeError = err instanceof Error ? err.message : String(err);
    } finally {
      finalizing = false;
    }
  }
</script>

<McpDisclaimerDialog
  open={showDisclaimer}
  onaccept={handleDisclaimerAccept}
  onclose={handleDisclaimerClose}
/>

<Dialog
  open={open && disclaimerPassed && !showDisclaimer}
  onclose={handleClose}
  size="lg"
  title={$t("integrations.wizard.title", {
    values: { name: server.title ?? server.name },
  })}
  data-testid="connector-wizard"
>
  <!-- Step progress indicator -->
  <div
    class="mb-6 flex flex-wrap items-center gap-1"
    role="list"
    aria-label={$t("integrations.wizard.step_progress")}
  >
    {#each visibleSteps as step, i (step)}
      <div class="flex items-center gap-1" role="listitem">
        <div
          class="flex h-5 w-5 shrink-0 items-center justify-center rounded-full text-[10px] font-medium transition-colors duration-150"
          class:bg-primary={i + 1 <= currentStep}
          class:text-primary-foreground={i + 1 <= currentStep}
          class:bg-muted={i + 1 > currentStep}
          class:text-muted-foreground={i + 1 > currentStep}
          aria-current={i + 1 === currentStep ? "step" : undefined}
        >
          {i + 1}
        </div>
        <span
          class="hidden text-xs transition-colors duration-150 sm:inline"
          class:text-foreground={i + 1 === currentStep}
          class:font-medium={i + 1 === currentStep}
          class:text-muted-foreground={i + 1 !== currentStep}
        >
          {$t(`integrations.wizard.step_${step}`)}
        </span>
        {#if i < visibleSteps.length - 1}
          <div class="mx-1.5 h-px w-4 bg-border"></div>
        {/if}
      </div>
    {/each}
  </div>

  <!-- Step content area -->
  <div class="min-h-[180px]">
    {#if currentStepName === "info"}
      <WizardStepInfo {server} />
    {:else if currentStepName === "auth"}
      {#if connectionMode === "remote"}
        <WizardStepAuth
          envVars={remoteHeadersAsEnvVars}
          enrichment={server.enrichment}
          values={envValues}
          onchange={handleEnvChange}
        />
      {:else if connectionMode === "package" && pkg}
        <WizardStepAuth
          envVars={pkg.environment_variables ?? []}
          enrichment={server.enrichment}
          values={envValues}
          onchange={handleEnvChange}
        />
      {/if}
    {:else if currentStepName === "params" && connectionMode === "package" && pkg}
      <WizardStepParams
        args={pkg.package_arguments ?? []}
        values={argValues}
        onchange={handleArgChange}
      />
    {:else if currentStepName === "test" && testConfig}
      <WizardStepTest config={testConfig} />
    {:else if currentStepName === "test" && !testConfig}
      <div class="space-y-2 rounded-md border border-border bg-muted/40 p-4" data-testid="wizard-step-test-unavailable">
        <p class="text-sm text-muted-foreground">
          {$t("integrations.wizard.test_unavailable")}
        </p>
      </div>
    {:else if currentStepName === "confirm" && canInstall}
      <WizardStepConfirm
        {approvalLevel}
        onchange={(level: "auto" | "ask" | "readonly") => {
          approvalLevel = level;
        }}
      />
    {:else if currentStepName === "confirm" && !canInstall}
      <div class="space-y-3 rounded-md border border-border bg-muted/40 p-4" data-testid="wizard-no-package">
        <p class="text-sm font-medium text-foreground">
          {$t("integrations.wizard.no_package_title")}
        </p>
        <p class="text-sm text-muted-foreground">
          {$t("integrations.wizard.no_package_body")}
        </p>
      </div>
    {/if}
  </div>

  {#if finalizeError}
    <p class="mt-3 text-sm text-destructive" data-testid="wizard-finalize-error">
      {finalizeError}
    </p>
  {/if}

  <!-- Navigation bar -->
  <div class="mt-6 flex items-center justify-between border-t border-border pt-4">
    <Button
      variant="outline"
      size="sm"
      onclick={goBack}
      disabled={currentStep === 1}
      data-testid="wizard-back-btn"
    >
      {$t("common.back")}
    </Button>

    {#if currentStep < totalSteps}
      <Button size="sm" onclick={goNext} data-testid="wizard-next-btn">
        {$t("integrations.wizard.next")}
      </Button>
    {:else}
      <Button
        size="sm"
        onclick={finalize}
        disabled={finalizing || !canInstall}
        data-testid="wizard-confirm-btn"
      >
        {finalizing
          ? $t("integrations.wizard.adding")
          : $t("integrations.wizard.confirm_button")}
      </Button>
    {/if}
  </div>
</Dialog>
