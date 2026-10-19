<script lang="ts">
  /**
   * First-launch step 5 - the first agent.
   *
   * Three cards, three ways to an agent that is ready to work before the
   * first launch ends:
   *
   *   - install an agent: the file chooser, then the preview every package
   *     gets, above this step; nothing is installed before its consent;
   *   - create one by describing the need: the conversation and the draft of
   *     the Agents screen, inside this step;
   *   - start from an example: the list of the examples the application
   *     proposes, one chosen, and that one only installed.
   *
   * The agent obtained is kept in `onboardingFirstAgent` for the steps that
   * follow. "Skip" keeps none and moves on. The decisions live in
   * `firstAgent.ts`; this component renders them.
   */
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import { open as openFilePicker } from "@tauri-apps/plugin-dialog";
  import { AlertCircle, Check, MessageSquarePlus, PackagePlus, Sparkles } from "lucide-svelte";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { Spinner } from "$lib/components/ui/progress";
  import { listExamplePackages, type ExamplePackage } from "$lib/ipc/packageExamples";
  import { onboardingFirstAgent } from "$lib/stores/onboarding";
  import {
    PACKAGE_INSTALLED_EVENT,
    openExamplePreview,
    openPackagePreview,
  } from "$lib/stores/packagePreview";
  import CreateAgent from "../agents/create/CreateAgent.svelte";
  import { currentCreation } from "../agents/create/creationDraft";
  import {
    FIRST_AGENT_CARDS,
    agentCreated,
    agentOfInstalledExample,
    agentOfPackage,
    exampleLines,
    exampleToInstall,
    installedPackageOf,
    viewOfCard,
    wayOfInstalled,
    type FirstAgentCard,
    type FirstAgentView,
  } from "./firstAgent";
  import "./onboarding-ai-setup.css";

  interface Props {
    /** What the step shows; the modal widens for the creation. */
    view?: FirstAgentView;
    /** On to the step after this one, with the agent kept or none. */
    onnext: () => void;
  }

  let { view = $bindable("cards"), onnext }: Props = $props();

  const CARD_ICONS = {
    install: PackagePlus,
    create: MessageSquarePlus,
    example: Sparkles,
  } as const;

  let examples = $state<ExamplePackage[]>([]);
  let examplesLoading = $state(false);
  let examplesError = $state<string | null>(null);
  let chosenExample = $state<string | null>(null);
  // The example the preview was asked for: what tells an example installed
  // from the list from a file opened through the first card.
  let askedExample: string | null = null;
  // Inline and not a toast: a toast is painted under the veil of the modal.
  let notice = $state<{ key: string; values?: Record<string, string> } | null>(null);

  const kept = $derived($onboardingFirstAgent);
  const lines = $derived(exampleLines(examples, chosenExample));
  const installable = $derived(exampleToInstall(examples, chosenExample));
  const chosenInstalled = $derived(agentOfInstalledExample(examples, chosenExample));

  function message(err: unknown): string {
    return err instanceof Error ? err.message : String(err);
  }

  async function loadExamples(): Promise<void> {
    examplesLoading = true;
    examplesError = null;
    try {
      examples = await listExamplePackages();
    } catch (err) {
      examples = [];
      examplesError = message(err);
    } finally {
      examplesLoading = false;
    }
  }

  async function pickPackageFile(): Promise<void> {
    try {
      const path = await openFilePicker({
        filters: [{ name: "Apollia", extensions: ["apo"] }],
        multiple: false,
      });
      if (typeof path !== "string") return;
      askedExample = null;
      openPackagePreview(path);
    } catch (err) {
      notice = { key: "onboarding.first_agent.pick_failed", values: { reason: message(err) } };
    }
  }

  function openCard(card: FirstAgentCard): void {
    notice = null;
    if (card.opens === "package-preview") {
      void pickPackageFile();
      return;
    }
    view = viewOfCard(card);
    if (view === "examples") void loadExamples();
  }

  function installChosenExample(): void {
    notice = null;
    if (chosenInstalled !== null) {
      // Already installed: nothing is installed again, its agent is kept.
      onboardingFirstAgent.retain(chosenInstalled);
      view = "cards";
      return;
    }
    if (installable === null) return;
    askedExample = installable;
    openExamplePreview(installable);
  }

  function onInstalled(event: Event): void {
    const installed = installedPackageOf((event as CustomEvent<unknown>).detail);
    if (installed === null) return;
    const agent = agentOfPackage(installed, wayOfInstalled(installed, askedExample));
    askedExample = null;
    if (agent === null) {
      notice = { key: "onboarding.first_agent.no_agent_in_package", values: { name: installed.name } };
      return;
    }
    notice = null;
    onboardingFirstAgent.retain(agent);
    view = "cards";
  }

  function skip(): void {
    onboardingFirstAgent.clear();
    onnext();
  }

  onMount(() => {
    // A creation left under way opens again where it was.
    if (view === "cards" && currentCreation("onboarding") !== null) view = "create";
    window.addEventListener(PACKAGE_INSTALLED_EVENT, onInstalled);
    return () => window.removeEventListener(PACKAGE_INSTALLED_EVENT, onInstalled);
  });
</script>

{#if view === "create"}
  <div class="flex min-h-0 flex-1 flex-col" data-testid="onboarding-first-agent" data-view="create">
    <CreateAgent
      embedded={true}
      onback={() => (view = "cards")}
      oncreated={(agent) => {
        onboardingFirstAgent.retain(agentCreated(agent));
        view = "cards";
      }}
    />
  </div>
{:else}
  <div class="ai-setup" data-testid="onboarding-first-agent" data-view={view}>
    {#if view === "examples"}
      <header class="setup-header">
        <h2 class="setup-title">{$t("onboarding.first_agent.examples_title")}</h2>
        <p class="setup-subtitle">{$t("onboarding.first_agent.examples_subtitle")}</p>
      </header>

      {#if examplesLoading}
        <p
          class="flex items-center gap-2 text-body-sm text-muted-foreground"
          data-testid="first-agent-examples-loading"
        >
          <Spinner size={16} />
          {$t("onboarding.first_agent.examples_loading")}
        </p>
      {:else if examplesError !== null}
        <p class="inline-error" role="alert" data-testid="first-agent-examples-error">
          <AlertCircle size={12} />
          {$t("onboarding.first_agent.examples_failed")}
          {examplesError}
        </p>
      {:else if lines.length === 0}
        <p class="text-body-sm text-muted-foreground" data-testid="first-agent-examples-empty">
          {$t("onboarding.first_agent.examples_empty")}
        </p>
      {:else}
        <div
          class="flex flex-col gap-2"
          role="radiogroup"
          aria-label={$t("onboarding.first_agent.examples_title")}
          data-testid="first-agent-examples"
        >
          {#each lines as line (line.name)}
            <button
              type="button"
              role="radio"
              aria-checked={line.chosen}
              class="source-card"
              class:source-card-chosen={line.chosen}
              onclick={() => (chosenExample = line.name)}
              data-testid="first-agent-example"
              data-example={line.name}
              data-chosen={line.chosen}
            >
              <Sparkles size={16} strokeWidth={2} aria-hidden="true" />
              <span class="source-card-text">
                <span class="flex items-center gap-2">
                  <span class="source-card-title">{line.title}</span>
                  {#if line.installed}
                    <Badge size="sm" variant="success" data-testid="first-agent-example-installed">
                      {$t("agents.examples.installed")}
                    </Badge>
                  {/if}
                </span>
                <span class="source-card-body">{line.mission}</span>
                {#if line.requires.length > 0}
                  <span class="source-card-body" data-testid="first-agent-example-requires">
                    {$t("agents.examples.requires", { values: { list: line.requires.join(", ") } })}
                  </span>
                {/if}
              </span>
            </button>
          {/each}
        </div>
      {/if}
    {:else}
      <header class="setup-header">
        <h2 class="setup-title">{$t("onboarding.first_agent.title")}</h2>
        <p class="setup-subtitle">{$t("onboarding.first_agent.subtitle")}</p>
      </header>

      {#if kept}
        <p
          class="flex items-start gap-2 rounded-lg border border-border bg-surface-1 px-3 py-2.5 text-body-sm text-foreground"
          role="status"
          data-testid="first-agent-kept"
          data-agent={kept.id}
          data-way={kept.way}
        >
          <Check size={14} strokeWidth={2.5} class="mt-0.5 shrink-0 text-success" aria-hidden="true" />
          <span>{$t("onboarding.first_agent.kept", { values: { name: kept.name } })}</span>
        </p>
      {/if}

      <div
        class="grid grid-cols-1 gap-2 sm:grid-cols-3"
        role="group"
        aria-label={$t("onboarding.first_agent.cards_label")}
        data-testid="first-agent-cards"
      >
        {#each FIRST_AGENT_CARDS as card (card.way)}
          {@const CardIcon = CARD_ICONS[card.way]}
          <button
            type="button"
            class="source-card"
            onclick={() => openCard(card)}
            data-testid={`first-agent-card-${card.way}`}
            data-opens={card.opens}
          >
            <CardIcon size={16} strokeWidth={2} aria-hidden="true" />
            <span class="source-card-text">
              <span class="source-card-title">{$t(card.titleKey)}</span>
              <span class="source-card-body">{$t(card.bodyKey)}</span>
            </span>
          </button>
        {/each}
      </div>
    {/if}

    {#if notice}
      <p class="inline-error" role="alert" data-testid="first-agent-notice">
        <AlertCircle size={12} />
        {$t(notice.key, { values: notice.values })}
      </p>
    {/if}

    <footer class="setup-footer">
      {#if view === "examples"}
        <Button
          variant="ghost"
          size="sm"
          class="btn-secondary"
          onclick={() => (view = "cards")}
          data-testid="first-agent-examples-back"
        >
          ← {$t("common.back")}
        </Button>
        <div class="footer-right">
          <Button
            variant="primary-gradient"
            size="default"
            onclick={installChosenExample}
            disabled={installable === null}
            data-testid="first-agent-example-install"
          >
            {chosenInstalled !== null
              ? $t("onboarding.first_agent.example_use")
              : $t("onboarding.first_agent.example_install")}
          </Button>
        </div>
      {:else}
        <span></span>
        <div class="footer-right">
          {#if kept}
            <Button
              variant="primary-gradient"
              size="default"
              onclick={onnext}
              data-testid="first-agent-continue"
            >
              {$t("common.continue")}
            </Button>
          {:else}
            <Button
              variant="ghost"
              size="sm"
              class="btn-tertiary"
              onclick={skip}
              data-testid="first-agent-skip"
            >
              {$t("onboarding.first_agent.skip")}
            </Button>
          {/if}
        </div>
      {/if}
    </footer>
  </div>
{/if}
