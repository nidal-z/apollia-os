<script lang="ts">
  import { onMount } from "svelte";
  import Sidebar from "./components/layout/Sidebar.svelte";
  import Main from "./components/layout/Main.svelte";
  import { createSSEConnection } from "$lib/stores/sse";

  onMount(() => {
    const cleanup = createSSEConnection();
    return cleanup;
  });
</script>

<div class="flex h-screen w-screen overflow-hidden">
  <Sidebar />
  <Main />
</div>
