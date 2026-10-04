<script lang="ts">
  import { onMount } from "svelte";
  import { guideContext, guideContextKey } from "$lib/stores/guideContext";
  import { companionStore } from "$lib/stores/companion";

  interface Props {
    children?: import("svelte").Snippet;
  }

  let { children }: Props = $props();

  let previousKey = $state<string>("");

  /**
   * Debounced context observer: waits 500 ms after the last change of route,
   * sub-screen or opened object before handing the guide its new context.
   * Rapid changes cancel previous timers so only the final one triggers an
   * IPC round-trip.
   */
  $effect(() => {
    const context = $guideContext;
    const timer = setTimeout(() => {
      const key = guideContextKey(context);
      if (key === previousKey) return;
      previousKey = key;
      void companionStore.updateContext(context);
    }, 500);
    return () => clearTimeout(timer);
  });

  onMount(() => {
    const context = $guideContext;
    previousKey = guideContextKey(context);
    void companionStore.updateContext(context);
  });
</script>

{@render children?.()}
