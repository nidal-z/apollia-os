<script lang="ts">
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import { currentRoute, goBack, goForward } from "$lib/stores/navigation";
  import { PageTransition } from "$lib/components/motion";
  import Topbar from "./Topbar.svelte";
  import { RuntimeStatusBanner } from "$lib/components/feedback";
  import { startRuntimeHealthMonitor } from "$lib/stores/runtimeHealth";
  import Dashboard from "../../../routes/Dashboard.svelte";
  import Agents from "../../../routes/Agents.svelte";
  import Tasks from "../../../routes/Tasks.svelte";
  import Inbox from "../../../routes/Inbox.svelte";
  import Llm from "../../../routes/Llm.svelte";
  import Automations from "../../../routes/Automations.svelte";
  import Memory from "../../../routes/Memory.svelte";
  import Notifications from "../../../routes/Notifications.svelte";
  import Observability from "../../../routes/Observability.svelte";
  import Chat from "../../../routes/Chat.svelte";
  import Settings from "../../../routes/Settings.svelte";
  import Transcriptions from "../../../routes/Transcriptions.svelte";
  import Connections from "../../../routes/Connections.svelte";
  import Projects from "../../../routes/Projects.svelte";
  import Design from "../../../routes/Design.svelte";
  import DesignMotion from "../../../routes/DesignMotion.svelte";
  import DesignEmptyStates from "../../../routes/DesignEmptyStates.svelte";
  import DesignDarkMode from "../../../routes/DesignDarkMode.svelte";

  const isDev = import.meta.env.DEV;

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
    // The banner is mounted here, so the poller that feeds it is armed here too.
    // It used to start in Chat.svelte's onMount, which meant a dead runtime went
    // unsignalled on the thirteen other routes, and leaving /chat mid-reconnect
    // froze the banner on its last state.
    const stopHealthMonitor = startRuntimeHealthMonitor();
    return () => {
      window.removeEventListener("keydown", handleKeydown);
      stopHealthMonitor();
    };
  });
</script>

<main
  id="main-content"
  class="flex min-w-0 flex-1 flex-col overflow-auto bg-background focus:outline-none"
  aria-label={$t("a11y.main_landmark")}
>
  <Topbar />

  <!-- Runtime health : global persistent banner across every route. -->
  <RuntimeStatusBanner />

  <!-- Route content: responsive padding plus a centred container. -->
  <div class="w-full flex-1 overflow-auto">
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
        {:else if $currentRoute === "inbox"}
          <Inbox />
        {:else if $currentRoute === "llm"}
          <Llm />
        {:else if $currentRoute === "automations"}
          <Automations />
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
          <Connections />
        {:else if $currentRoute === "settings"}
          <Settings />
        {:else if isDev && $currentRoute === "design"}
          <Design />
        {:else if isDev && $currentRoute === "design-motion"}
          <DesignMotion />
        {:else if isDev && $currentRoute === "design-empty-states"}
          <DesignEmptyStates />
        {:else if isDev && $currentRoute === "design-dark-mode"}
          <DesignDarkMode />
        {:else}
          <!-- No branch matched. Without this fallback the window renders empty,
               which is what an unknown route produced: the companion maps
               `/onboarding` (onboarding is a modal, not a route), so its button
               opened a blank screen. Any route the switch does not know now
               lands on the dashboard instead of nothing. -->
          <Dashboard />
        {/if}
      </PageTransition>
    {/key}
  </div>
</main>
