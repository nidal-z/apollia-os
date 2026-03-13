<script lang="ts">
  import { currentRoute } from "$lib/stores/navigation";
  import Agents from "../../routes/Agents.svelte";
  import Tasks from "../../routes/Tasks.svelte";
  import Approvals from "../../routes/Approvals.svelte";
  import Llm from "../../routes/Llm.svelte";
  import Triggers from "../../routes/Triggers.svelte";
</script>

<main class="flex-1 overflow-auto bg-background p-6">
  {#if $currentRoute === "agents"}
    <Agents />
  {:else if $currentRoute === "tasks"}
    <Tasks />
  {:else if $currentRoute === "approvals"}
    <Approvals />
  {:else if $currentRoute === "llm"}
    <Llm />
  {:else if $currentRoute === "triggers"}
    <Triggers />
  {/if}
</main>
