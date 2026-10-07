<script lang="ts">
  /**
   * LogsWindow - the logs of the local server in a window of their own, the
   * same pane the developer screen shows. Closing the window hands the logs
   * back to that screen.
   */
  import { onMount } from "svelte";
  import { Tooltip } from "bits-ui";
  import { ToastContainer } from "$lib/components/ui/toast";
  import { developerMode, hydrateDeveloperMode } from "$lib/stores/developerMode";
  import LogsPane from "../components/developer/LogsPane.svelte";

  onMount(() => {
    void hydrateDeveloperMode();
  });
</script>

<Tooltip.Provider delayDuration={200}>
  <main class="flex h-screen flex-col bg-background text-foreground">
    {#if $developerMode}
      <LogsPane windowed />
    {/if}
  </main>
  <ToastContainer />
</Tooltip.Provider>
