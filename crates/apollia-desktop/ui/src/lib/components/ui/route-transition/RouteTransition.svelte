<script lang="ts">
  /**
   * RouteTransition - wraps route content in the canonical enter/leave swap.
   *
   * A thin convenience over `contentIn` / `contentOut` from
   * `$lib/design/routeTransition`. Place inside a `{#key routeId}` so each
   * navigation re-runs the transition. Reduced-motion aware via `rm()`.
   */
  import type { Snippet } from "svelte";
  import { fly } from "svelte/transition";
  import { contentIn, contentOut } from "$lib/design/routeTransition";

  interface Props {
    children: Snippet;
  }

  let { children }: Props = $props();
</script>

<div class="h-full w-full" in:fly={contentIn()} out:fly={contentOut()}>
  {@render children()}
</div>
