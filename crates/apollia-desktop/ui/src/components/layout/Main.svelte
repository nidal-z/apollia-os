<script lang="ts">
  import { fade } from "svelte/transition";
  import { onMount } from "svelte";
  import { currentRoute, canGoBack, canGoForward, goBack, goForward } from "$lib/stores/navigation";
  import { ChevronLeft, ChevronRight } from "lucide-svelte";
  import Dashboard from "../../routes/Dashboard.svelte";
  import Agents from "../../routes/Agents.svelte";
  import Tasks from "../../routes/Tasks.svelte";
  import Approvals from "../../routes/Approvals.svelte";
  import Llm from "../../routes/Llm.svelte";
  import Triggers from "../../routes/Triggers.svelte";
  import Pipelines from "../../routes/Pipelines.svelte";
  import Memory from "../../routes/Memory.svelte";
  import Notifications from "../../routes/Notifications.svelte";
  import Observability from "../../routes/Observability.svelte";
  import Chat from "../../routes/Chat.svelte";
  import Settings from "../../routes/Settings.svelte";
  import Onboarding from "../../routes/Onboarding.svelte";
  import Transcriptions from "../../routes/Transcriptions.svelte";

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

<main class="flex flex-1 flex-col overflow-auto bg-background">
  <!-- Navigation bar -->
  <div class="flex items-center gap-1 px-8 pt-6 pb-2">
    <button
      class="h-7 w-7 inline-flex items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-muted hover:text-foreground disabled:opacity-30 disabled:pointer-events-none"
      onclick={goBack}
      disabled={!$canGoBack}
      aria-label="Back (Cmd+[)"
      data-testid="nav-back"
    >
      <ChevronLeft size={16} strokeWidth={1.75} />
    </button>
    <button
      class="h-7 w-7 inline-flex items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-muted hover:text-foreground disabled:opacity-30 disabled:pointer-events-none"
      onclick={goForward}
      disabled={!$canGoForward}
      aria-label="Forward (Cmd+])"
      data-testid="nav-forward"
    >
      <ChevronRight size={16} strokeWidth={1.75} />
    </button>
  </div>

  <!-- Route content -->
  <div class="flex-1 px-8 pb-8">
    {#key $currentRoute}
      <div transition:fade={{ duration: 150 }}>
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
        {:else if $currentRoute === "llm"}
          <Llm />
        {:else if $currentRoute === "triggers"}
          <Triggers />
        {:else if $currentRoute === "pipelines"}
          <Pipelines />
        {:else if $currentRoute === "memory"}
          <Memory />
        {:else if $currentRoute === "transcriptions"}
          <Transcriptions />
        {:else if $currentRoute === "notifications"}
          <Notifications />
        {:else if $currentRoute === "observability"}
          <Observability />
        {:else if $currentRoute === "settings"}
          <Settings />
        {:else if $currentRoute === "onboarding"}
          <Onboarding />
        {/if}
      </div>
    {/key}
  </div>
</main>
