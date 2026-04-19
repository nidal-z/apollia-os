<script lang="ts">
  import { onMount } from "svelte";
  import { get } from "svelte/store";
  import { isLoading, t } from "svelte-i18n";
  import Sidebar from "./components/layout/Sidebar.svelte";
  import Main from "./components/layout/Main.svelte";
  import { SkipToContent } from "$lib/components/layout";
  import KeyboardHintOverlay from "./components/common/KeyboardHintOverlay.svelte";
  import OnboardingWelcome from "./components/onboarding/OnboardingWelcome.svelte";
  import ProfileSelector from "./components/onboarding/ProfileSelector.svelte";
  import OnboardingAiSetup from "./components/onboarding/OnboardingAiSetup.svelte";
  import OnboardingAcquaintance from "./components/onboarding/OnboardingAcquaintance.svelte";
  import OnboardingGuidedTour from "./components/onboarding/OnboardingGuidedTour.svelte";
  import OnboardingGraduation from "./components/onboarding/OnboardingGraduation.svelte";
  import OnboardingResumeBar from "./components/onboarding/OnboardingResumeBar.svelte";
  import { ToastContainer } from "$lib/components/ui/toast";
  import ExtractionNotifier from "./components/chat/ExtractionNotifier.svelte";
  import CompanionContextProvider from "./components/companion/CompanionContextProvider.svelte";
  import CompanionPanel from "./components/companion/CompanionPanel.svelte";
  import { companionStore } from "$lib/stores/companion";
  import { Tooltip } from "bits-ui";
  import { createSSEConnection } from "$lib/stores/sse";
  import { initTheme, themeMode } from "$lib/stores/theme";
  import { onboardingStore } from "$lib/stores/onboarding";
  import { navigateTo } from "$lib/stores/navigation";
  import type { OnboardingPhase } from "$lib/types";
  import { Command } from "$lib/components/ui/command";
  import type { CommandPaletteGroup } from "$lib/components/ui/command";
  import {
    commandPaletteOpen,
    commands,
    registerCommand,
  } from "$lib/stores/commandPalette";
  import {
    LayoutDashboard,
    Bot,
    ListChecks,
    MessageSquare,
    Settings,
    Plug,
    Moon,
    HelpCircle,
    Play,
    Square,
  } from "lucide-svelte";

  // Phases that represent an in-progress (but not yet started) onboarding.
  const IN_PROGRESS_PHASES = new Set<OnboardingPhase>([
    "profile_choice",
    "ai_setup",
    "acquaintance",
    "guided_tour",
    "graduation",
  ]);

  // Step numbers for the resume bar label (1-based, 5 phases after welcome).
  const PHASE_STEP: Partial<Record<OnboardingPhase, number>> = {
    profile_choice: 1,
    ai_setup: 2,
    acquaintance: 3,
    guided_tour: 4,
    graduation: 5,
  };
  const TOTAL_STEPS = 5;

  type OnboardingDisplayMode =
    | "phase"        // Active onboarding — render the phase component fullscreen.
    | "main_with_bar" // Resumed at startup — show main app + non-intrusive bar.
    | "main";        // User dismissed the bar — show main app without bar.

  let ready = $state(false);
  let displayMode = $state<OnboardingDisplayMode>("phase");

  const isMac = typeof navigator !== "undefined" && navigator.platform.includes("Mac");

  // Seed commands: navigation, agent lifecycle stubs, theme, help.
  // Registered once App mounts; cleanup unregisters.
  function buildSeedCommands() {
    return [
      {
        id: "nav.dashboard",
        label: get(t)("command.seed.nav_dashboard"),
        group: get(t)("command.group.navigation"),
        icon: LayoutDashboard,
        keywords: ["home", "dashboard"],
        action: () => navigateTo("dashboard"),
      },
      {
        id: "nav.agents",
        label: get(t)("command.seed.nav_agents"),
        group: get(t)("command.group.navigation"),
        icon: Bot,
        keywords: ["agents", "assistants"],
        action: () => navigateTo("agents"),
      },
      {
        id: "nav.tasks",
        label: get(t)("command.seed.nav_tasks"),
        group: get(t)("command.group.navigation"),
        icon: ListChecks,
        keywords: ["tasks", "activity"],
        action: () => navigateTo("tasks"),
      },
      {
        id: "nav.chat",
        label: get(t)("command.seed.nav_chat"),
        group: get(t)("command.group.navigation"),
        icon: MessageSquare,
        keywords: ["chat", "conversation"],
        action: () => navigateTo("chat"),
      },
      {
        id: "nav.integrations",
        label: get(t)("command.seed.nav_integrations"),
        group: get(t)("command.group.navigation"),
        icon: Plug,
        keywords: ["integrations", "mcp", "connections"],
        action: () => navigateTo("integrations"),
      },
      {
        id: "nav.settings",
        label: get(t)("command.seed.nav_settings"),
        group: get(t)("command.group.navigation"),
        icon: Settings,
        keywords: ["settings", "preferences"],
        action: () => navigateTo("settings"),
      },
      {
        id: "agents.start_all",
        label: get(t)("command.seed.agents_start_all"),
        group: get(t)("command.group.agents"),
        icon: Play,
        keywords: ["start", "run", "all", "agents"],
        action: () => {
          window.dispatchEvent(new CustomEvent("apollia:agents:start-all"));
        },
      },
      {
        id: "agents.stop_all",
        label: get(t)("command.seed.agents_stop_all"),
        group: get(t)("command.group.agents"),
        icon: Square,
        keywords: ["stop", "halt", "all", "agents"],
        action: () => {
          window.dispatchEvent(new CustomEvent("apollia:agents:stop-all"));
        },
      },
      {
        id: "theme.toggle_dark",
        label: get(t)("command.seed.toggle_dark"),
        group: get(t)("command.group.preferences"),
        icon: Moon,
        keywords: ["dark", "light", "theme", "mode"],
        action: () => {
          const current = get(themeMode);
          themeMode.set(current === "dark" ? "light" : "dark");
        },
      },
      {
        id: "help.palette_docs",
        label: get(t)("command.seed.palette_docs"),
        group: get(t)("command.group.help"),
        icon: HelpCircle,
        shortcut: ["?"],
        keywords: ["help", "shortcuts", "palette", "docs"],
        action: () => {
          window.dispatchEvent(
            new KeyboardEvent("keydown", { key: "?", bubbles: true }),
          );
        },
      },
    ];
  }

  // Grouping for the Command component — order determines visual order.
  const paletteGroups = $derived.by<CommandPaletteGroup[]>(() => {
    const list = $commands;
    const buckets = new Map<string, CommandPaletteGroup>();
    for (const cmd of list) {
      const g = buckets.get(cmd.group);
      if (g) {
        g.items.push(cmd);
      } else {
        buckets.set(cmd.group, { label: cmd.group, items: [cmd] });
      }
    }
    return Array.from(buckets.values());
  });

  function handleGlobalKeydown(event: KeyboardEvent) {
    const mod = isMac ? event.metaKey : event.ctrlKey;
    if (mod && !event.shiftKey && !event.altKey && event.key.toLowerCase() === "k") {
      event.preventDefault();
      commandPaletteOpen.update((v) => !v);
    }
  }

  onMount(() => {
    initTheme();
    const cleanup = createSSEConnection();

    // Design showcase pages — internal-facing, not advertised in the sidebar.
    if (typeof window !== "undefined") {
      if (window.location.hash === "#design") {
        navigateTo("design");
      } else if (window.location.hash === "#motion") {
        navigateTo("design-motion");
      } else if (window.location.hash === "#design-empty-states") {
        navigateTo("design-empty-states");
      } else if (window.location.hash === "#design-dark-mode") {
        navigateTo("design-dark-mode");
      }
    }

    onboardingStore
      .refreshState()
      .catch(() => {
        // Backend unreachable at startup — default state (phase: "welcome") is used.
      })
      .finally(() => {
        ready = true;
        const { phase } = get(onboardingStore);
        if (IN_PROGRESS_PHASES.has(phase)) {
          displayMode = "main_with_bar";
        }
      });

    void companionStore.initFromMemory();

    window.addEventListener("keydown", handleGlobalKeydown);

    // Defer seed registration until svelte-i18n has loaded labels,
    // otherwise `get(t)(...)` returns the raw key. `isLoading` flips
    // to false once `en.json` / `fr.json` are parsed.
    const stopI18n = isLoading.subscribe((loading) => {
      if (!loading) {
        const seeds = buildSeedCommands();
        unregisterSeeds = registerCommand(...seeds);
        stopI18n();
      }
    });

    return () => {
      cleanup?.();
      window.removeEventListener("keydown", handleGlobalKeydown);
      unregisterSeeds?.();
      stopI18n();
    };
  });

  let unregisterSeeds: (() => void) | undefined;

  function handlePaletteExecute(id: string) {
    // Telemetry hook — currently logs; real backend wiring lives in the
    // observability pipeline (`apollia.ui.command.executed`).
    console.debug("[command-palette] executed", id);
  }

  function handleResume(): void {
    displayMode = "phase";
  }

  function handleDismissBar(): void {
    displayMode = "main";
  }
</script>

<Tooltip.Provider delayDuration={200}>
  <!-- Keyboard-first entry points (US-SP42-007 F.77 / E.45) — visible on
       focus only. Wait until svelte-i18n has loaded before rendering,
       otherwise `$t(...)` throws "Cannot format a message without
       first setting the initial locale." -->
  {#if !$isLoading}
    <SkipToContent />
    <KeyboardHintOverlay />
  {/if}
  {#if $isLoading || !ready}
    <div
      class="flex h-screen w-screen items-center justify-center bg-background text-foreground"
      data-testid="app-loading"
    >
      <p class="text-sm text-muted-foreground">{$isLoading ? "Chargement…" : $t("common.loading")}</p>
    </div>
  {:else if $onboardingStore.completed || $onboardingStore.phase === "done"}
    <CompanionContextProvider>
      <div class="flex h-screen w-screen overflow-hidden" data-testid="app-main">
        <Sidebar />
        <Main />
      </div>
      <CompanionPanel />
    </CompanionContextProvider>
    <ExtractionNotifier />
    <ToastContainer />
  {:else if $onboardingStore.phase === "welcome"}
    <OnboardingWelcome />
  {:else if (displayMode === "main_with_bar" || displayMode === "main") && IN_PROGRESS_PHASES.has($onboardingStore.phase)}
    <CompanionContextProvider>
      <div class="flex h-screen w-screen flex-col overflow-hidden" data-testid="app-main">
        {#if displayMode === "main_with_bar"}
          <OnboardingResumeBar
            currentStep={PHASE_STEP[$onboardingStore.phase] ?? 1}
            totalSteps={TOTAL_STEPS}
            onresume={handleResume}
            ondismiss={handleDismissBar}
          />
        {/if}
        <div class="flex min-h-0 flex-1 overflow-hidden">
          <Sidebar />
          <Main />
        </div>
      </div>
      <CompanionPanel />
    </CompanionContextProvider>
    <ExtractionNotifier />
    <ToastContainer />
  {:else if $onboardingStore.phase === "profile_choice"}
    <ProfileSelector />
  {:else if $onboardingStore.phase === "ai_setup"}
    <OnboardingAiSetup />
  {:else if $onboardingStore.phase === "acquaintance"}
    <OnboardingAcquaintance />
  {:else if $onboardingStore.phase === "guided_tour"}
    <CompanionContextProvider>
      <div class="flex h-screen w-screen overflow-hidden" data-testid="app-main">
        <Sidebar />
        <Main />
      </div>
      <CompanionPanel />
    </CompanionContextProvider>
    <OnboardingGuidedTour />
  {:else if $onboardingStore.phase === "graduation"}
    <OnboardingGraduation />
  {:else}
    <CompanionContextProvider>
      <div class="flex h-screen w-screen overflow-hidden" data-testid="app-main">
        <Sidebar />
        <Main />
      </div>
      <CompanionPanel />
    </CompanionContextProvider>
    <ExtractionNotifier />
    <ToastContainer />
  {/if}

  <!-- Global Cmd+K / Ctrl+K palette. Always mounted so it survives route
       changes and onboarding display-mode transitions. -->
  <Command
    bind:open={$commandPaletteOpen}
    groups={paletteGroups}
    onexecute={handlePaletteExecute}
  />
</Tooltip.Provider>
