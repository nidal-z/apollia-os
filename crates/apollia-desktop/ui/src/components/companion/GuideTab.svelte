<script lang="ts">
  /**
   * The Guide tab of the detail sidebar. From the top: where the user is and
   * what the screen is for, the actions the guide offers from here, then the
   * guide's conversation, whose answers carry their own action buttons. The
   * session is created the first time the tab is shown with a model able to
   * answer. With none, the documentation page of the screen takes the place
   * of the conversation.
   */
  import { untrack } from "svelte";
  import { t } from "svelte-i18n";
  import {
    companionStore,
    COMPANION_SESSION_TIMEOUT_MS,
  } from "$lib/stores/companion";
  import { detailSidebar, guideVisible } from "$lib/stores/detailSidebar";
  import { chatTargets, refreshChatTargets } from "$lib/stores/chatTargets";
  import { developerMode } from "$lib/stores/developerMode";
  import { guideContext, guideContextKey } from "$lib/stores/guideContext";
  import { navigateTo, type Route } from "$lib/stores/navigation";
  import { guideModelState } from "$lib/apolliaGuide/guideModel";
  import { guideScreenView } from "$lib/apolliaGuide/screenContexts";
  import { addToast } from "$lib/components/ui/toast";
  import ChatConversation from "../chat/ChatConversation.svelte";
  import CompanionSessionSkeleton from "./CompanionSessionSkeleton.svelte";
  import CompanionErrorState from "./CompanionErrorState.svelte";
  import GuideDocumentation from "./GuideDocumentation.svelte";
  import GuideScreenContext from "./GuideScreenContext.svelte";

  const view = $derived(guideScreenView($guideContext, $t, $developerMode));
  // The models are read again each time the tab is shown and each time the
  // screen changes under it: a model installed or a server connected since
  // the last read brings the conversation back. A read that could not be
  // made leaves the conversation its chance: it says itself what fails.
  let modelsAsked = $state(false);
  const model = $derived.by(() => {
    const state = guideModelState($chatTargets);
    return state === "unknown" && modelsAsked ? "available" : state;
  });

  $effect(() => {
    if (!$guideVisible) return;
    void guideContextKey($guideContext);
    untrack(() => {
      void refreshChatTargets().then(() => {
        modelsAsked = true;
      });
    });
  });

  const sessionId = $derived($companionStore.sessionId);
  const route = $derived($companionStore.currentRoute);
  const sessionStatus = $derived($companionStore.sessionStatus);
  const sessionError = $derived($companionStore.error);

  const showSkeleton = $derived(
    sessionStatus === "connecting" || sessionStatus === "creating",
  );

  let timeoutToastTimer: ReturnType<typeof setTimeout> | null = null;

  async function ensureSession() {
    if (sessionId || showSkeleton) return;
    if (timeoutToastTimer) clearTimeout(timeoutToastTimer);
    // Soft warning toast at the timeout boundary - the store also surfaces a
    // structured error state when the underlying IPC call eventually fails.
    timeoutToastTimer = setTimeout(() => {
      addToast($t("companion.session.timeout_toast_title"), "info");
    }, COMPANION_SESSION_TIMEOUT_MS);
    try {
      await companionStore.createSession(route);
    } catch {
      // Error state is rendered from the store; no rethrow needed.
    } finally {
      if (timeoutToastTimer) clearTimeout(timeoutToastTimer);
      timeoutToastTimer = null;
    }
  }

  $effect(() => {
    if (
      $guideVisible &&
      model === "available" &&
      !sessionId &&
      sessionStatus === "idle" &&
      !sessionError
    ) {
      untrack(() => void ensureSession());
    }
  });

  function handleRetry() {
    companionStore.clearError();
    void ensureSession();
  }

  function handleErrorClose() {
    companionStore.clearError();
    detailSidebar.close();
  }

  function handleOpenSettings() {
    if (!sessionError) {
      navigateTo("settings");
      return;
    }
    const map: Record<string, Route> = {
      network: "integrations",
      permissions: "authorizations",
      timeout: "llm",
      internal: "settings",
    };
    navigateTo(map[sessionError.kind] ?? "settings");
  }
</script>

<div
  class="flex h-full min-h-0 flex-col overflow-hidden"
  role="tabpanel"
  aria-label={$t("detailSidebar.tab_guide")}
  data-testid="companion-panel"
  data-model={model}
>
  {#if view}
    <div class="max-h-[55%] shrink-0 overflow-y-auto border-b border-border">
      <GuideScreenContext {view} />
    </div>
  {/if}
  {#if model === "none" && view}
    <GuideDocumentation id={view.id} />
  {:else}
    <div class="flex min-h-0 flex-1 flex-col overflow-hidden">
      {#if sessionError}
        <CompanionErrorState
          error={sessionError}
          onRetry={handleRetry}
          onOpenSettings={handleOpenSettings}
          onClose={handleErrorClose}
        />
      {:else if showSkeleton || model === "unknown"}
        <CompanionSessionSkeleton status={sessionStatus} />
      {:else if sessionId}
        <ChatConversation
          {sessionId}
          onclose={() => detailSidebar.close()}
          embedded={true}
          hideConfig={true}
          suggestions={view ? [view.question] : undefined}
        />
      {:else}
        <div class="flex h-full items-center justify-center text-muted-foreground">
          <span class="text-sm">{$t("companion.no_session")}</span>
        </div>
      {/if}
    </div>
  {/if}
</div>
