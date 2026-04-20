<script lang="ts">
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import { currentRoute, canGoBack, canGoForward, goBack, goForward } from "$lib/stores/navigation";
  import { sidebarState, layoutActions } from "$lib/stores/layout";
  import { ChevronLeft, ChevronRight, Menu } from "lucide-svelte";
  import { PageTransition } from "$lib/components/motion";
  import Dashboard from "../../routes/Dashboard.svelte";
  import Agents from "../../routes/Agents.svelte";
  import Tasks from "../../routes/Tasks.svelte";
  import Approvals from "../../routes/Approvals.svelte";
  import Inbox from "../../routes/Inbox.svelte";
  import Llm from "../../routes/Llm.svelte";
  import Triggers from "../../routes/Triggers.svelte";
  import Automations from "../../routes/Automations.svelte";
  import Pipelines from "../../routes/Pipelines.svelte";
  import Memory from "../../routes/Memory.svelte";
  import Notifications from "../../routes/Notifications.svelte";
  import Observability from "../../routes/Observability.svelte";
  import Chat from "../../routes/Chat.svelte";
  import Settings from "../../routes/Settings.svelte";
  import Onboarding from "../../routes/Onboarding.svelte";
  import Transcriptions from "../../routes/Transcriptions.svelte";
  import Integrations from "../../routes/Integrations.svelte";
  import Connections from "../../routes/Connections.svelte";
  import { uiMode } from "$lib/stores/mode";
  import Projects from "../../routes/Projects.svelte";
  import Design from "../../routes/Design.svelte";
  import DesignMotion from "../../routes/DesignMotion.svelte";
  import DesignEmptyStates from "../../routes/DesignEmptyStates.svelte";
  import DesignDarkMode from "../../routes/DesignDarkMode.svelte";

  onMount(() => {
    function handleKeydown(event: KeyboardEvent) {
      if ((event.metaKey || event.ctrlKey) && event.key === "[") {
        event.preventDefault();
        goBack();
      } else if ((event.metaKey || event.ctrlKey) && event.key === "]") {
        event.preventDefault();
        goForward();
      }
    }
    window.addEventListener("keydown", handleKeydown);
    return () => window.removeEventListener("keydown", handleKeydown);
  });
</script>

<main
  id="main-content"
  class="flex min-w-0 flex-1 flex-col overflow-auto bg-background focus:outline-none"
  aria-label={$t("a11y.main_landmark")}
>
  <!-- Navigation bar -->
  <div class="flex items-center gap-1 px-4 sm:px-6 lg:px-8 pt-4 pb-2">
    {#if $sidebarState === "drawer"}
      <button
        class="inline-flex min-h-11 min-w-11 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
        onclick={() => layoutActions.openDrawer()}
        aria-label={$t("nav.open_sidebar")}
        aria-haspopup="dialog"
        data-testid="topbar-sidebar-toggle"
      >
        <Menu size={20} strokeWidth={1.75} />
      </button>
    {/if}
    <button
      class="inline-flex min-h-11 min-w-11 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-muted hover:text-foreground disabled:opacity-30 disabled:pointer-events-none"
      onclick={goBack}
      disabled={!$canGoBack}
      aria-label={$t("a11y.back_shortcut")}
      data-testid="nav-back"
    >
      <ChevronLeft size={18} strokeWidth={1.75} />
    </button>
    <button
      class="inline-flex min-h-11 min-w-11 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-muted hover:text-foreground disabled:opacity-30 disabled:pointer-events-none"
      onclick={goForward}
      disabled={!$canGoForward}
      aria-label={$t("a11y.forward_shortcut")}
      data-testid="nav-forward"
    >
      <ChevronRight size={18} strokeWidth={1.75} />
    </button>
  </div>

  <!-- Route content : padding responsive + conteneur défini par chaque route. -->
  <div class="flex-1 px-4 sm:px-6 lg:px-8 pb-8">
    {#key $currentRoute}
      <PageTransition>
        {#if $currentRoute === "dashboard"}
          <Dashboard />
        {:else if $currentRoute === "agents"}
          <Agents />
        {:else if $currentRoute === "tasks"}
          <Tasks />
        {:else if $currentRoute === "chat"}
          <Chat />
        {:else if $currentRoute === "approvals"}
          <Approvals />
        {:else if $currentRoute === "inbox"}
          <Inbox />
        {:else if $currentRoute === "llm"}
          <Llm />
        {:else if $currentRoute === "triggers"}
          <Triggers />
        {:else if $currentRoute === "automations"}
          <Automations />
        {:else if $currentRoute === "pipelines"}
          <Pipelines />
        {:else if $currentRoute === "projects"}
          <Projects />
        {:else if $currentRoute === "memory"}
          <Memory />
        {:else if $currentRoute === "transcriptions"}
          <Transcriptions />
        {:else if $currentRoute === "notifications"}
          <Notifications />
        {:else if $currentRoute === "observability"}
          <Observability />
        {:else if $currentRoute === "integrations"}
          {#if $uiMode === "operator"}
            <Connections />
          {:else}
            <Integrations />
          {/if}
        {:else if $currentRoute === "settings"}
          <Settings />
        {:else if $currentRoute === "onboarding"}
          <Onboarding />
        {:else if $currentRoute === "design"}
          <Design />
        {:else if $currentRoute === "design-motion"}
          <DesignMotion />
        {:else if $currentRoute === "design-empty-states"}
          <DesignEmptyStates />
        {:else if $currentRoute === "design-dark-mode"}
          <DesignDarkMode />
        {/if}
      </PageTransition>
    {/key}
  </div>
</main>
