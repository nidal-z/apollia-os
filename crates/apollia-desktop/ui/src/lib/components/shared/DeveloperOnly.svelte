<!--
  Renders its children only when the developer mode is on.
  Thin wrapper over `$developerMode` to keep callsites readable and give a
  single seam to test against.
-->
<script lang="ts">
  import type { Snippet } from "svelte";
  import { developerMode } from "$lib/stores/developerMode";

  interface Props {
    children?: Snippet;
  }

  let { children }: Props = $props();
</script>

{#if $developerMode}
  {@render children?.()}
{/if}
