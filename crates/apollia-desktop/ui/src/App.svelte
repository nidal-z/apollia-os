<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { isLoading, t } from "svelte-i18n";
  import Sidebar from "./components/layout/Sidebar.svelte";
  import Main from "./components/layout/Main.svelte";
  import { SkipToContent } from "$lib/components/layout";
  import KeyboardHintOverlay from "./components/common/KeyboardHintOverlay.svelte";
  import OnboardingModal from "./components/onboarding/OnboardingModal.svelte";
  import { ToastContainer } from "$lib/components/ui/toast";
  import ExtractionNotifier from "./components/chat/ExtractionNotifier.svelte";
  import CompanionContextProvider from "./components/companion/CompanionContextProvider.svelte";
  import CompanionPanel from "./components/companion/CompanionPanel.svelte";
  import { companionStore } from "$lib/stores/companion";
  import { Tooltip } from "bits-ui";
  import { createSSEConnection } from "$lib/stores/sse";
  import { initTheme } from "$lib/stores/theme";
  import { navigateTo } from "$lib/stores/navigation";
  import { onboardingModalOpen } from "$lib/stores/onboarding";
  import type { OnboardingState } from "$lib/types";
  import { CommandPalette } from "./components/command-palette";
  import { installGlobalShortcuts } from "$lib/keyboard/globalShortcuts";
  import { openNewTaskRequested } from "$lib/stores/tasks";
  import { llmBackends } from "$lib/stores/sse";
  import { get } from "svelte/store";

  let ready = $state(false);
  let prevLlmCount = 0;

  const isMac = typeof navigator !== "undefined" && navigator.platform.includes("Mac");

  function handleAncillaryKeydown(event: KeyboardEvent) {
    const mod = isMac ? event.metaKey : event.ctrlKey;
    // A.1.13 — Cmd/Ctrl+T: create a new task (navigates to /tasks and opens dialog).
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
    const cleanup = createSSEConnection();
    const disposeShortcuts = installGlobalShortcuts();

    // Design showcase pages — DEV-only, gated out of release builds.
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

    // Initial check — handles the case where OnboardingRequired was emitted
    // before this listener attached (e.g. during the splash/loading window).
    void invoke<OnboardingState>("get_onboarding_state")
      .then((state) => {
        if (shouldOpenOnboarding(state)) {
          onboardingModalOpen.set(true);
        }
      })
      .catch(() => {
        // Backend unreachable at startup — modal stays closed; the supervisor
        // will retry the OnboardingRequired event once ready.
      })
      .finally(() => {
        ready = true;
      });

    // Live channel — supervisor emits OnboardingRequired at first launch and
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
      disposeShortcuts();
      unlistenRuntime?.();
      unsubscribeLlm();
    };
  });

  function handleOnboardingClose(): void {
    onboardingModalOpen.set(false);
  }
</script>

<Tooltip.Provider delayDuration={200}>
  <!-- Keyboard-first entry points (F.77 / E.45) — visible on
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
    <ExtractionNotifier />
    <ToastContainer />
    {#if $onboardingModalOpen}
      <OnboardingModal onclose={handleOnboardingClose} />
    {/if}
  {/if}

  <!-- Global Cmd+K / Ctrl+Shift+P palette. Always mounted so it survives
       route changes and onboarding display-mode transitions. -->
  <CommandPalette />
</Tooltip.Provider>
