<script lang="ts">
  /**
   * The Guide tab of the detail sidebar: the guide's chat session, opened on
   * the context of the current screen. The session is created the first time
   * the tab is shown.
   */
  import { untrack } from "svelte";
  import { t } from "svelte-i18n";
  import {
    companionStore,
    COMPANION_SESSION_TIMEOUT_MS,
  } from "$lib/stores/companion";
  import { detailSidebar, guideVisible } from "$lib/stores/detailSidebar";
  import { navigateTo, type Route } from "$lib/stores/navigation";
  import { addToast } from "$lib/components/ui/toast";
  import ChatConversation from "../chat/ChatConversation.svelte";
  import CompanionSessionSkeleton from "./CompanionSessionSkeleton.svelte";
  import CompanionErrorState from "./CompanionErrorState.svelte";

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
    if ($guideVisible && !sessionId && sessionStatus === "idle" && !sessionError) {
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
>
  {#if sessionError}
    <CompanionErrorState
      error={sessionError}
      onRetry={handleRetry}
      onOpenSettings={handleOpenSettings}
      onClose={handleErrorClose}
    />
  {:else if showSkeleton}
    <CompanionSessionSkeleton status={sessionStatus} />
  {:else if sessionId}
    <ChatConversation
      {sessionId}
      onclose={() => detailSidebar.close()}
      embedded={true}
      hideConfig={true}
    />
  {:else}
    <div class="flex h-full items-center justify-center text-muted-foreground">
      <span class="text-sm">{$t("companion.no_session")}</span>
    </div>
  {/if}
</div>
