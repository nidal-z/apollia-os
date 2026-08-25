<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { isLoading, t } from "svelte-i18n";
  import { Main, Sidebar } from "$lib/components/app";
  import { SkipToContent } from "$lib/components/layout";
  import KeyboardHintOverlay from "./components/common/KeyboardHintOverlay.svelte";
  import OnboardingModal from "./components/onboarding/OnboardingModal.svelte";
  import TourHost from "$lib/components/tour/TourHost.svelte";
  import { ToastContainer } from "$lib/components/ui/toast";
  import PlanModeHost from "$lib/components/operator/PlanModeHost.svelte";
  import CompanionContextProvider from "./components/companion/CompanionContextProvider.svelte";
  import CompanionPanel from "./components/companion/CompanionPanel.svelte";
  import { companionStore } from "$lib/stores/companion";
  import { Tooltip } from "bits-ui";
  import { createSSEConnection } from "$lib/stores/sse";
  import { initTheme } from "$lib/stores/theme";
  import { hydratePlanModeDefault } from "$lib/stores/planModeSetting";
  import { navigateTo } from "$lib/stores/navigation";
  import { onboardingModalOpen } from "$lib/stores/onboarding";
  import type { OnboardingState } from "$lib/types";
  import { CommandPalette } from "./components/command-palette";
  import { mountShortcutDispatcher } from "$lib/navigation/shortcutDispatcher";
  import { registerGlobalShortcutHandlers } from "$lib/navigation/globalShortcutHandlers";
  import { openNewTaskRequested } from "$lib/stores/tasks";
  import { llmBackends } from "$lib/stores/sse";
  import { get } from "svelte/store";
  import type { AutomationBoot } from "$lib/automation/types";

  let ready = $state(false);
  let prevLlmCount = 0;

  const isMac = typeof navigator !== "undefined" && navigator.platform.includes("Mac");

  function handleAncillaryKeydown(event: KeyboardEvent) {
    const mod = isMac ? event.metaKey : event.ctrlKey;
    // Cmd/Ctrl+T: create a new task (navigates to /tasks and opens dialog).
    if (mod && !event.shiftKey && !event.altKey && event.key.toLowerCase() === "t") {
      const target = event.target as HTMLElement | null;
      const isEditing =
        target &&
        (target.tagName === "INPUT" ||
          target.tagName === "TEXTAREA" ||
          target.isContentEditable);
      if (isEditing) return;
      event.preventDefault();
      navigateTo("tasks");
      openNewTaskRequested.set(Date.now());
      return;
    }
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

  function shouldOpenOnboarding(state: OnboardingState): boolean {
    return !state.completed && !state.skipped && state.started_at === null;
  }

  onMount(() => {
    initTheme();
    void hydratePlanModeDefault();
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
    void invoke<{ enabled?: boolean }>("get_stt_config")
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
      void invoke<AutomationBoot | null>("automation_script")
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
    void invoke<OnboardingState>("get_onboarding_state")
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
    // OnboardingCompleted once the agent writes onboarding.completed_at.
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
      const justBecameAvailable = prevLlmCount === 0 && count > 0;
      prevLlmCount = count;
      if (!justBecameAvailable) return;
      void invoke<OnboardingState>("get_onboarding_state")
        .then((state) => {
          if (shouldOpenOnboarding(state)) {
            onboardingModalOpen.set(true);
          }
        })
        .catch(() => {});
    });

    window.addEventListener("keydown", handleAncillaryKeydown);

    return () => {
      cleanup?.();
      window.removeEventListener("keydown", handleAncillaryKeydown);
      disposeGlobalHandlers();
      disposeShortcutDispatcher();
      unlistenRuntime?.();
      unsubscribeLlm();
    };
  });

  function handleOnboardingClose(): void {
    onboardingModalOpen.set(false);
    // The guided tour used to own this call, which meant a user who never
    // finished the tour was never marked onboarded. The tour is now optional
    // and launched from the dashboard, so completing the modal is what counts.
    void invoke("mark_onboarded").catch(() => {});
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
      <div class="flex h-screen w-screen overflow-hidden" data-testid="app-main">
        <Sidebar />
        <Main />
      </div>
      <CompanionPanel />
    </CompanionContextProvider>
    <PlanModeHost />
    <ToastContainer />
    {#if $onboardingModalOpen}
      <OnboardingModal onclose={handleOnboardingClose} />
    {/if}
    <TourHost />
  {/if}

  <!-- Global Cmd+K / Ctrl+Shift+P palette. Always mounted so it survives
       route changes and onboarding display-mode transitions. -->
  <CommandPalette />
</Tooltip.Provider>
