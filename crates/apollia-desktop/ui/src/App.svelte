<script lang="ts">
  import { onMount } from "svelte";
    import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { isLoading, t } from "svelte-i18n";
  import { Main, Sidebar } from "$lib/components/app";
  import { SkipToContent } from "$lib/components/layout";
  import KeyboardHintOverlay from "./components/common/KeyboardHintOverlay.svelte";
  import OnboardingModal from "./components/onboarding/OnboardingModal.svelte";
  import OnboardingFirstTask from "./components/onboarding/OnboardingFirstTask.svelte";
  import ProjectMigrationNotice from "./components/projectMigration/ProjectMigrationNotice.svelte";
  import PackagePreviewDialog from "./components/packages/PackagePreviewDialog.svelte";
  import { ToastContainer } from "$lib/components/ui/toast";
  import PlanModeHost from "$lib/components/operator/PlanModeHost.svelte";
  import CompanionContextProvider from "./components/companion/CompanionContextProvider.svelte";
  import { companionStore } from "$lib/stores/companion";
  import { Tooltip } from "bits-ui";
  import { createSSEConnection } from "$lib/stores/sse";
  import { initTheme } from "$lib/stores/theme";
  import { initChatAppearance, navPosition } from "$lib/stores/chatAppearance";
  import { navSide } from "$lib/layout/navSide";
  import { hydratePlanModeDefault } from "$lib/stores/planModeSetting";
  import { hydrateChatAutoNaming } from "$lib/stores/chatAutoNaming";
  import { hydrateChatBehaviours } from "$lib/stores/chatBehaviours";
  import { hydrateGeneralBehaviours } from "$lib/stores/generalBehaviours";
  import { startGenerationSpeedListener } from "$lib/chat/autoName";
  import { hydrateDeveloperMode } from "$lib/stores/developerMode";
  import { hydrateDeveloperSettings } from "$lib/stores/developerSettings";
  import { navigateTo } from "$lib/stores/navigation";
  import { onboardingModalOpen } from "$lib/stores/onboarding";
  import { engineArrivalAsksAgain, shouldOpenOnboarding } from "$lib/onboarding/reopening";
  import { CommandPalette } from "./components/command-palette";
  import { mountShortcutDispatcher } from "$lib/navigation/shortcutDispatcher";
  import { registerGlobalShortcutHandlers } from "$lib/navigation/globalShortcutHandlers";
  import { llmBackends } from "$lib/stores/sse";
  import { get } from "svelte/store";
  import { getSttConfig } from "$lib/ipc/models";
  import { getOnboardingState, markOnboarded } from "$lib/ipc/onboarding";
  import { automationScript } from "$lib/ipc/system";
  import { mountPackageOpening } from "$lib/packages/packageOpening";

  let ready = $state(false);
  let prevLlmCount = 0;
  // Set when the first-launch modal is closed, finished or put off: a language
  // model that arrives later in this run does not open it again.
  let onboardingClosed = false;

  const isMac = typeof navigator !== "undefined" && navigator.platform.includes("Mac");

  function handleAncillaryKeydown(event: KeyboardEvent) {
    const mod = isMac ? event.metaKey : event.ctrlKey;
    // Cmd/Ctrl+Shift+A: toggle the artifacts tab of the context drawer.
    if (mod && event.shiftKey && !event.altKey && event.key.toLowerCase() === "a") {
      event.preventDefault();
      void (async () => {
        const { contextDrawerOpen, contextDrawerActiveTab } = await import(
          "$lib/stores/chatLayout"
        );
        contextDrawerActiveTab.set("artifacts");
        contextDrawerOpen.set(true);
      })();
    }
  }

  onMount(() => {
    initTheme();
    initChatAppearance();
    void hydratePlanModeDefault();
    void hydrateDeveloperMode();
    void hydrateDeveloperSettings();
    void hydrateChatAutoNaming();
    void hydrateChatBehaviours();
    void hydrateGeneralBehaviours();
    let unlistenSpeed: (() => void) | null = null;
    void startGenerationSpeedListener()
      .then((fn) => {
        unlistenSpeed = fn;
      })
      .catch(() => {});
    const cleanup = createSSEConnection();
    // Phase-0 shortcut dispatcher: the single window keydown listener that
    // routes chords to handlers surfaces register via `registerShortcutHandler`.
    const disposeShortcutDispatcher = mountShortcutDispatcher();
    // Register the global-scope handlers (command palette, companion focus)
    // against that dispatcher. Contextual chords stay owned by their surfaces.
    const disposeGlobalHandlers = registerGlobalShortcutHandlers();

    // When dictation is enabled, prime the microphone permission at boot so the
    // global hotkey (which starts a native cpal capture the WebView cannot
    // precede) reads real audio instead of silence. The grant is app-wide and
    // persists once accepted; ensureMicPermission never prompts twice.
    void getSttConfig()
      .then(async (cfg) => {
        if (cfg?.enabled) {
          const { ensureMicPermission } = await import("$lib/stt/micPermission");
          await ensureMicPermission();
        }
      })
      .catch(() => {});

    // Design showcase pages - DEV-only, gated out of release builds.
    if (import.meta.env.DEV && typeof window !== "undefined") {
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

    // Dev-only gestural automaton - when APOLLIA_AUTOMATION points at a script,
    // the backend returns it and we drive the real app against its testids.
    // The import.meta.env.DEV guard tree-shakes the whole block (and the runner
    // chunk) out of release builds. Scripts own their own readiness waits.
    if (import.meta.env.DEV) {
      void automationScript()
        .then(async (boot) => {
          if (!boot) return;
          const { runAutomation } = await import("$lib/automation/runner");
          await runAutomation(boot.script, boot.allowDestructive);
        })
        .catch((e) => {
          console.error("[automation] boot failed", e);
        });
    }

    // Initial check - handles the case where OnboardingRequired was emitted
    // before this listener attached (e.g. during the splash/loading window).
    void getOnboardingState()
      .then((state) => {
        if (shouldOpenOnboarding(state)) {
          onboardingModalOpen.set(true);
        }
      })
      .catch(() => {
        // Backend unreachable at startup - modal stays closed; the supervisor
        // will retry the OnboardingRequired event once ready.
      })
      .finally(() => {
        ready = true;
      });

    // Live channel - supervisor emits OnboardingRequired at first launch and
    // OnboardingCompleted once the phase machine reaches its last phase.
    let unlistenRuntime: UnlistenFn | null = null;
    void listen<{ category: string; event_type: string }>(
      "runtime-event",
      (evt) => {
        if (evt.payload.category !== "onboarding-changed") return;
        if (evt.payload.event_type === "OnboardingRequired") {
          onboardingModalOpen.set(true);
        } else if (evt.payload.event_type === "OnboardingCompleted") {
          onboardingModalOpen.set(false);
        }
      },
    ).then((fn) => {
      unlistenRuntime = fn;
    });

    void companionStore.initFromMemory();

    // When the user finishes configuring an LLM after the onboarding modal
    // showed its "configure LLM" gate (post factory-reset / fresh install),
    // reopen the modal automatically so the agent chat can resume.
    prevLlmCount = get(llmBackends).length;
    const unsubscribeLlm = llmBackends.subscribe((list) => {
      const count = list.length;
      const asksAgain = engineArrivalAsksAgain({
        previous: prevLlmCount,
        count,
        closedThisRun: onboardingClosed,
      });
      prevLlmCount = count;
      if (!asksAgain) return;
      void getOnboardingState()
        .then((state) => {
          // The modal may have been closed while the state was read.
          if (!onboardingClosed && shouldOpenOnboarding(state)) {
            onboardingModalOpen.set(true);
          }
        })
        .catch(() => {});
    });

    // Opened again, by a reset or from the settings: a new run of the modal.
    const unsubscribeOnboarding = onboardingModalOpen.subscribe((open) => {
      if (open) onboardingClosed = false;
    });

    window.addEventListener("keydown", handleAncillaryKeydown);

    // A package file opened from the system, or dropped on the window, opens
    // its preview.
    const stopPackageOpening = mountPackageOpening();

    return () => {
      cleanup?.();
      stopPackageOpening();
      window.removeEventListener("keydown", handleAncillaryKeydown);
      disposeGlobalHandlers();
      disposeShortcutDispatcher();
      unlistenRuntime?.();
      unlistenSpeed?.();
      unsubscribeLlm();
      unsubscribeOnboarding();
    };
  });

  function handleOnboardingClose(): void {
    onboardingClosed = true;
    onboardingModalOpen.set(false);
    // Closing the modal, whether finished or skipped, is what marks the user
    // as onboarded: no later step is required to count as done.
    void markOnboarded().catch(() => {});
  }
</script>

<Tooltip.Provider delayDuration={200}>
  <!-- Keyboard-first entry points - visible on
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
  {:else}
    <CompanionContextProvider>
      <div
        class="flex h-screen w-screen overflow-hidden {navSide($navPosition).shellRow}"
        data-testid="app-main"
        data-nav-position={$navPosition}
      >
        <Sidebar />
        <Main />
      </div>
    </CompanionContextProvider>
    <PlanModeHost />
    <!-- The preview of a `.apo` package, opened from any screen by its path. -->
    <PackagePreviewDialog />
    <ToastContainer />
    <!-- What the first launch closes on: a conversation, or a batch. -->
    <OnboardingFirstTask />
    {#if $onboardingModalOpen}
      <OnboardingModal onclose={handleOnboardingClose} />
    {:else}
      <!-- Shown once, after the former projects became sources. -->
      <ProjectMigrationNotice />
    {/if}
  {/if}

  <!-- Global Cmd+K / Ctrl+Shift+P palette. Always mounted so it survives
       route changes and onboarding display-mode transitions. -->
  <CommandPalette />
</Tooltip.Provider>
