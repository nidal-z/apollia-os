<script lang="ts">
  import { t } from "svelte-i18n";
  import { invoke } from "@tauri-apps/api/core";
  import {
    BookOpen,
    KeyRound,
    PlugZap,
    Sparkles,
  } from "lucide-svelte";
  import { Dialog } from "$lib/components/ui/dialog";
  import { Button } from "$lib/components/ui/button";
  import { Stepper } from "$lib/components/ui/stepper";
  import WizardStepDisclaimer, {
    DISCLAIMER_ITEMS,
    isDisclaimerVersionAccepted,
    recordDisclaimerAcceptance,
  } from "./WizardStepDisclaimer.svelte";
  import WizardStepAuth from "./WizardStepAuth.svelte";
  import WizardStepTest from "./WizardStepTest.svelte";
  import WizardStepCoaching from "./WizardStepCoaching.svelte";
  import {
    isFirstConnectionTourDone,
    markFirstConnectionTourDone,
  } from "$lib/tour/FirstConnectionTour";
  import FirstConnectionTourRunner from "./FirstConnectionTourRunner.svelte";
  import type {
    McpServerConfigInput,
    RegistryEnvVarView,
    RegistryServerView,
  } from "$lib/types";

  interface Props {
    server: RegistryServerView;
    open: boolean;
    /** When true, expose "bypass test" escape hatch + skip disclaimer re-prompt logic. */
    builder?: boolean;
    onclose: () => void;
    oncomplete: () => void;
    /** Called with a pre-filled prompt when the user clicks "Try" on a coaching card. */
    ontryprompt?: (prompt: string) => void;
  }

  let {
    server,
    open,
    builder = false,
    onclose,
    oncomplete,
    ontryprompt = () => {},
  }: Props = $props();

  type WizardStep = "disclaimer" | "auth" | "test" | "coaching";

  // ── Disclaimer state (step 1) ───────────────────────────────────────────────
  let disclaimerChecks = $state<Record<string, boolean>>({});
  let disclaimerVersionOk = $state(false);

  const disclaimerComplete = $derived(
    DISCLAIMER_ITEMS.every((key) => {
      const short = key.replace("i18n:integrations.disclaimer.items.", "");
      return disclaimerChecks[short] === true;
    }),
  );

  // ── Auth / Test / Coaching state ────────────────────────────────────────────
  let currentStepIndex = $state(0);
  let envValues = $state<Record<string, string>>({});
  let approvalLevel = $state<"auto" | "ask" | "readonly">("ask");
  let testSucceeded = $state(false);
  let testBypassed = $state(false);
  let finalizing = $state(false);
  let finalizeError = $state<string | null>(null);
  let showFirstTour = $state(false);

  $effect(() => {
    if (open) {
      currentStepIndex = 0;
      envValues = {};
      approvalLevel = "ask";
      testSucceeded = false;
      testBypassed = false;
      finalizeError = null;
      // Pre-check disclaimer if current version is already accepted.
      void isDisclaimerVersionAccepted().then((v) => {
        disclaimerVersionOk = v;
        if (v) {
          disclaimerChecks = {
            code_on_machine: true,
            external_data: true,
            revocable: true,
            read_capabilities: true,
          };
        } else {
          disclaimerChecks = {};
        }
      });
    }
  });

  // ── Connection mode derivation (remote takes precedence over package) ───────
  const remote = $derived(server.remotes[0] ?? null);
  const pkg = $derived(server.packages?.[0] ?? null);
  const connectionMode = $derived<"remote" | "package" | null>(
    remote ? "remote" : pkg ? "package" : null,
  );

  const remoteHeadersAsEnvVars = $derived.by((): RegistryEnvVarView[] => {
    if (!remote) return [];
    return remote.headers.map((h) => ({
      name: h.name,
      description: h.description,
      is_required: h.isRequired,
      is_secret: h.isSecret,
    }));
  });

  const authEnvVars = $derived.by((): RegistryEnvVarView[] => {
    if (connectionMode === "remote") return remoteHeadersAsEnvVars;
    if (connectionMode === "package") return pkg?.environment_variables ?? [];
    return [];
  });

  const hasWriteTools = $derived.by((): boolean => {
    // Heuristic fallback until backend exposes `enrichment.capabilities.has_write`:
    // assume a server has write tools unless it is an explicit read-only category.
    const ro = new Set(["search", "analytics"]);
    const cat = server.enrichment?.category ?? server.category ?? "";
    return !ro.has(cat);
  });

  const canInstall = $derived(connectionMode !== null);

  // ── Ordered wizard steps ────────────────────────────────────────────────────
  const steps: readonly WizardStep[] = [
    "disclaimer",
    "auth",
    "test",
    "coaching",
  ] as const;

  const stepperSteps = $derived([
    { label: $t("integrations.wizard.step_disclaimer"), icon: BookOpen },
    { label: $t("integrations.wizard.step_auth"), icon: KeyRound },
    { label: $t("integrations.wizard.step_test"), icon: PlugZap },
    { label: $t("integrations.wizard.step_coaching"), icon: Sparkles },
  ]);

  const currentStep = $derived(steps[currentStepIndex]);
  const totalSteps = steps.length;

  // Required-field completion for the auth step: every required var must have a value.
  const authComplete = $derived.by((): boolean => {
    const required = authEnvVars.filter((v) => v.is_required);
    return required.every((v) => (envValues[v.name] ?? "").trim() !== "");
  });

  const canAdvance = $derived.by((): boolean => {
    switch (currentStep) {
      case "disclaimer":
        return disclaimerComplete;
      case "auth":
        return authComplete;
      case "test":
        return testSucceeded || testBypassed;
      case "coaching":
        return canInstall && !finalizing;
      default:
        return false;
    }
  });

  /**
   * Backend validate_name() in apollia-mcp enforces `[a-z0-9_-]+` strictly,
   * so identifiers like `@modelcontextprotocol/server-filesystem` or
   * `com.figma/mcp-cloud` must be sanitised before being sent. We:
   * - lowercase the string
   * - replace every non-`[a-z0-9_-]` character with `-`
   * - collapse runs of `-` and trim them at the edges
   * - fall back to `mcp-server` if the result is empty.
   */
  function sanitizeServerName(raw: string): string {
    const cleaned = raw
      .toLowerCase()
      .replace(/[^a-z0-9_-]+/g, "-")
      .replace(/-+/g, "-")
      .replace(/^-|-$/g, "");
    return cleaned.length > 0 ? cleaned : "mcp-server";
  }

  // ── Config builder ──────────────────────────────────────────────────────────
  function buildConfig(forTest: boolean): McpServerConfigInput | null {
    // Prefer the operator label (e.g. "Local Files") over the registry
    // identifier (e.g. "@modelcontextprotocol/server-filesystem"). The
    // operator label is the human-facing card name; using it as the server
    // name keeps the installed-card label, the runtime logs, and the
    // `mcp:<server>/<tool>` invocation prefix consistent and readable.
    let nameSource = server.enrichment?.operator_label ?? server.title ?? server.name;
    if (!nameSource || nameSource.trim().length === 0) {
      nameSource = server.name;
    }
    const safeName = sanitizeServerName(nameSource);
    if (connectionMode === "remote" && remote) {
      const env: Record<string, string> = {};
      for (const header of remote.headers) {
        env[header.name] =
          header.isSecret && !forTest
            ? `\${APOLLIA_SECRET:${header.name}}`
            : (envValues[header.name] ?? "");
      }
      return {
        name: safeName,
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
        name: safeName,
        command: pkg.runtime_hint ?? "npx",
        args: [
          pkg.identifier,
          ...(pkg.package_arguments ?? []).map((_, i) => String(i)),
        ],
        env,
        transport: pkg.transport_type ?? "stdio",
        requires_approval: approvalLevel === "ask",
        tags: [],
      };
    }
    return null;
  }

  const testConfig = $derived(buildConfig(true));
  const builtConfig = $derived(buildConfig(false));

  // ── Navigation ──────────────────────────────────────────────────────────────
  async function goNext(): Promise<void> {
    if (!canAdvance) return;
    if (currentStep === "disclaimer" && !disclaimerVersionOk) {
      await recordDisclaimerAcceptance();
      disclaimerVersionOk = true;
    }
    if (currentStepIndex < totalSteps - 1) currentStepIndex += 1;
  }

  function goBack(): void {
    if (currentStepIndex > 0) currentStepIndex -= 1;
  }

  function handleDisclaimerChange(key: string, value: boolean): void {
    disclaimerChecks = { ...disclaimerChecks, [key]: value };
  }

  function handleEnvChange(key: string, value: string): void {
    envValues = { ...envValues, [key]: value };
  }

  function handleTestSuccess(): void {
    testSucceeded = true;
  }

  function handleFixAuth(): void {
    testSucceeded = false;
    testBypassed = false;
    currentStepIndex = steps.indexOf("auth");
  }

  function handleBypass(): void {
    if (!builder) return;
    testBypassed = true;
  }

  function handleRequestClose(): void {
    if (
      currentStepIndex > 0 &&
      currentStep !== "coaching" &&
      !confirm($t("integrations.wizard.confirm_exit"))
    ) {
      return;
    }
    onclose();
  }

  async function finalize(): Promise<void> {
    if (!builtConfig) return;
    finalizing = true;
    finalizeError = null;
    try {
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
      oncomplete();
      // Launch the 4-step first-connection tour on the first successful setup.
      if (!isFirstConnectionTourDone()) {
        showFirstTour = true;
      } else {
        onclose();
      }
    } catch (err: unknown) {
      finalizeError = err instanceof Error ? err.message : String(err);
    } finally {
      finalizing = false;
    }
  }

  function handleTourClose(): void {
    markFirstConnectionTourDone();
    showFirstTour = false;
    onclose();
  }
</script>

<Dialog
  open={open && !showFirstTour}
  onclose={handleRequestClose}
  size="lg"
  title={$t("integrations.wizard.title", {
    values: { name: server.title ?? server.name },
  })}
  data-testid="connector-wizard"
>
  <!-- Stepper -->
  <div class="mb-6">
    <Stepper steps={stepperSteps} current={currentStepIndex} />
  </div>

  <!-- Step content -->
  <div class="min-h-[220px]">
    {#if currentStep === "disclaimer"}
      <WizardStepDisclaimer
        checks={disclaimerChecks}
        onchange={handleDisclaimerChange}
      />
    {:else if currentStep === "auth"}
      <WizardStepAuth
        envVars={authEnvVars}
        enrichment={server.enrichment}
        values={envValues}
        onchange={handleEnvChange}
      />
    {:else if currentStep === "test"}
      {#if testConfig}
        <WizardStepTest
          config={testConfig}
          allowBypass={builder}
          onsuccess={handleTestSuccess}
          onfixauth={handleFixAuth}
          onbypass={handleBypass}
        />
      {:else}
        <div
          class="space-y-2 rounded-md border border-border bg-muted/40 p-4"
          data-testid="wizard-step-test-unavailable"
        >
          <p class="text-sm text-muted-foreground">
            {$t("integrations.wizard.test_unavailable")}
          </p>
        </div>
      {/if}
    {:else if currentStep === "coaching"}
      {#if canInstall}
        <WizardStepCoaching
          serverName={server.name}
          serverTitle={server.title}
          {hasWriteTools}
          {approvalLevel}
          onchange={(l) => (approvalLevel = l)}
          ontry={ontryprompt}
        />
      {:else}
        <div
          class="space-y-3 rounded-md border border-border bg-muted/40 p-4"
          data-testid="wizard-no-package"
        >
          <p class="text-sm font-medium text-foreground">
            {$t("integrations.wizard.no_package_title")}
          </p>
          <p class="text-sm text-muted-foreground">
            {$t("integrations.wizard.no_package_body")}
          </p>
        </div>
      {/if}
    {/if}
  </div>

  {#if finalizeError}
    <p
      class="mt-3 text-sm text-destructive"
      data-testid="wizard-finalize-error"
    >
      {finalizeError}
    </p>
  {/if}

  <!-- Navigation bar -->
  <div
    class="mt-6 flex items-center justify-between border-t border-border pt-4 sticky bottom-0 bg-background"
  >
    <Button
      variant="outline"
      size="sm"
      onclick={goBack}
      disabled={currentStepIndex === 0}
      data-testid="wizard-back-btn"
    >
      {$t("common.back")}
    </Button>

    {#if currentStep !== "coaching"}
      <Button
        variant="primary-solid"
        size="sm"
        onclick={() => void goNext()}
        disabled={!canAdvance}
        data-testid="wizard-next-btn"
      >
        {$t("integrations.wizard.next")}
      </Button>
    {:else}
      <Button
        variant="primary-gradient"
        size="sm"
        onclick={() => void finalize()}
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

{#if showFirstTour}
  <FirstConnectionTourRunner onclose={handleTourClose} />
{/if}
