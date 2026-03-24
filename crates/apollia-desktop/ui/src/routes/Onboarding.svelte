<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import { Sparkles, ArrowRight, Clock } from "lucide-svelte";
  import { navigateTo } from "$lib/stores/navigation";
  import { onboardingStore } from "$lib/stores/onboarding";
  import { pendingChatSessionId } from "$lib/stores/chat";
  import type { ChatSessionSummary, CreateSessionRequest } from "$lib/types";

  let starting = $state(false);

  async function startOnboarding(): Promise<void> {
    if (starting) return;
    starting = true;
    try {
      const request: CreateSessionRequest = {
        mode: "agent",
        agent_name: "onboarding-agent",
      };
      const session = await invoke<ChatSessionSummary>(
        "create_chat_session",
        { request },
      );
      pendingChatSessionId.set(session.id);
      navigateTo("chat");
    } catch {
      starting = false;
    }
  }

  function skipOnboarding(): void {
    onboardingStore.setSkipped();
    navigateTo("dashboard");
  }
</script>

<div
  class="flex min-h-[calc(100vh-5rem)] items-center justify-center"
  data-testid="onboarding-screen"
>
  <div class="animate-fade-in flex w-full max-w-lg flex-col items-center">
    <!-- Logo with glow pulse -->
    <div
      class="mb-8 flex h-20 w-20 items-center justify-center rounded-2xl bg-gradient-to-br from-primary to-secondary animate-glow-pulse"
      data-testid="onboarding-logo"
    >
      <Sparkles size={36} strokeWidth={1.5} class="text-white" />
    </div>

    <!-- Glass card -->
    <div class="glass-card glass-border w-full rounded-2xl p-10 text-center">
      <h1
        class="mb-3 text-2xl font-bold text-foreground"
        data-testid="onboarding-title"
      >
        {$t("onboarding_welcome.title")}
      </h1>
      <p class="mb-8 text-sm leading-relaxed text-muted-foreground">
        {$t("onboarding_welcome.subtitle")}
      </p>

      <div class="flex flex-col gap-3">
        <button
          class="inline-flex w-full items-center justify-center gap-2 rounded-xl bg-primary px-6 py-3 text-sm font-semibold text-primary-foreground transition-colors hover:bg-primary/90 disabled:opacity-50"
          data-testid="onboarding-configure"
          onclick={startOnboarding}
          disabled={starting}
        >
          {#if starting}
            <span class="h-4 w-4 animate-spin rounded-full border-2 border-primary-foreground/30 border-t-primary-foreground"></span>
            {$t("onboarding_welcome.starting")}
          {:else}
            <ArrowRight size={16} strokeWidth={2} />
            {$t("onboarding_welcome.configure")}
          {/if}
        </button>

        <button
          class="inline-flex w-full items-center justify-center gap-2 rounded-xl border border-border bg-transparent px-6 py-3 text-sm font-medium text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
          data-testid="onboarding-skip"
          onclick={skipOnboarding}
        >
          <Clock size={16} strokeWidth={1.75} />
          {$t("onboarding_welcome.skip")}
        </button>
      </div>
    </div>
  </div>
</div>
