<script lang="ts">
  import { onMount } from "svelte";
  import { currentRoute } from "$lib/stores/navigation";
  import { companionStore } from "$lib/stores/companion";

  interface Props {
    children?: import("svelte").Snippet;
  }

  let { children }: Props = $props();

  // Track the previous route to avoid redundant IPC calls.
  let previousRoute = $state<string>("");

  $effect(() => {
    const route = $currentRoute;
    if (route === previousRoute) return;
    previousRoute = route;

    companionStore.updateRoute(route);

    // Pre-fetch context so it is ready when the panel opens.
    companionStore.fetchContext(route).catch(() => {
      // Non-critical — companion still works without pre-loaded context.
    });
  });

  onMount(() => {
    // Sync the initial route on mount.
    const route = $currentRoute;
    companionStore.updateRoute(route);
    companionStore.fetchContext(route).catch(() => {});
  });
</script>

{@render children?.()}
