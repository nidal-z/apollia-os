<script lang="ts">
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import { currentRoute, goBack, goForward } from "$lib/stores/navigation";
  import { developerMode } from "$lib/stores/developerMode";
  import { reachableRoute } from "$lib/navigation/developerRoutes";
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
  import SettingsWindow from "../../../components/settings/SettingsWindow.svelte";
  import Transcriptions from "../../../routes/Transcriptions.svelte";
  import Connections from "../../../routes/Connections.svelte";
  import Projects from "../../../routes/Projects.svelte";
  import Design from "../../../routes/Design.svelte";
  import DesignMotion from "../../../routes/DesignMotion.svelte";
  import DesignEmptyStates from "../../../routes/DesignEmptyStates.svelte";
  import DesignDarkMode from "../../../routes/DesignDarkMode.svelte";

  const isDev = import.meta.env.DEV;

  // A developer screen is out of reach with the developer mode off, whatever
  // asked for it (a link, the guide, the automaton): the router shows the
  // default route, and the store follows so the sidebar marks the right entry.
  const shownRoute = $derived(reachableRoute($currentRoute, $developerMode));
  $effect(() => {
    if (shownRoute !== $currentRoute) currentRoute.set(shownRoute);
  });

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
    {#key shownRoute}
      <PageTransition>
        {#if shownRoute === "dashboard"}
          <Dashboard />
        {:else if shownRoute === "agents"}
          <Agents />
        {:else if shownRoute === "tasks"}
          <Tasks />
        {:else if shownRoute === "chat"}
          <Chat />
        {:else if shownRoute === "inbox"}
          <Inbox />
        {:else if shownRoute === "llm"}
          <Llm />
        {:else if shownRoute === "automations"}
          <Automations />
        {:else if shownRoute === "projects"}
          <Projects />
        {:else if shownRoute === "memory"}
          <Memory />
        {:else if shownRoute === "transcriptions"}
          <Transcriptions />
        {:else if shownRoute === "notifications"}
          <Notifications />
        {:else if shownRoute === "observability"}
          <Observability />
        {:else if shownRoute === "integrations"}
          <Connections />
        {:else if isDev && shownRoute === "design"}
          <Design />
        {:else if isDev && shownRoute === "design-motion"}
          <DesignMotion />
        {:else if isDev && shownRoute === "design-empty-states"}
          <DesignEmptyStates />
        {:else if isDev && shownRoute === "design-dark-mode"}
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

  <!-- The settings open above the screen, which stays mounted underneath: the
       `settings` route opens this window rather than switching the screen. -->
  <SettingsWindow />
</main>
