<script lang="ts">
  import { onMount } from "svelte";
  import { currentRoute } from "$lib/stores/navigation";
  import { companionStore } from "$lib/stores/companion";

  interface Props {
    children?: import("svelte").Snippet;
  }

  let { children }: Props = $props();

  let previousRoute = $state<string>("");

  /**
   * Debounced route observer: waits 500 ms after the last navigation before
   * fetching the new context. Rapid navigations cancel previous timers so
   * only the final destination triggers an IPC round-trip.
   */
  $effect(() => {
    const route = $currentRoute;
    const timer = setTimeout(() => {
      if (route === previousRoute) return;
      previousRoute = route;
      companionStore.updateRoute(route);
      void companionStore.updateContext(route);
    }, 500);
    return () => clearTimeout(timer);
  });

  onMount(() => {
    const route = $currentRoute;
    previousRoute = route;
    companionStore.updateRoute(route);
    void companionStore.updateContext(route);
  });
</script>

{@render children?.()}
