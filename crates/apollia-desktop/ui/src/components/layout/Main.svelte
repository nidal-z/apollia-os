<script lang="ts">
  import { currentRoute } from "$lib/stores/navigation";
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
  import Settings from "../../routes/Settings.svelte";
</script>

<main class="flex-1 overflow-auto bg-background p-6">
  {#if $currentRoute === "dashboard"}
    <Dashboard />
  {:else if $currentRoute === "agents"}
    <Agents />
  {:else if $currentRoute === "tasks"}
    <Tasks />
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
  {:else if $currentRoute === "notifications"}
    <Notifications />
  {:else if $currentRoute === "observability"}
    <Observability />
  {:else if $currentRoute === "settings"}
    <Settings />
  {/if}
</main>
