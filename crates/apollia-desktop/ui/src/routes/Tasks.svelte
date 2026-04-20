<script lang="ts">
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import TaskList from "../components/tasks/TaskList.svelte";
  import TaskDetail from "../components/tasks/TaskDetail.svelte";
  import TasksFab from "../components/tasks/TasksFab.svelte";
  import { uiMode } from "$lib/stores/mode";
  import { openNewTaskRequested } from "$lib/stores/tasks";

  let selectedTaskId = $state<string | null>(null);
  let detailOpen = $state(false);

  const title = $derived($uiMode === "operator" ? $t("tasks.title_operator") : $t("tasks.title"));
  const subtitle = $derived(
    $uiMode === "operator" ? $t("tasks.subtitle_operator") : $t("tasks.subtitle"),
  );

  function handleSelectTask(taskId: string) {
    selectedTaskId = taskId;
    detailOpen = true;
  }

  function handleCloseDetail() {
    detailOpen = false;
  }

  function handleFabClick() {
    openNewTaskRequested.set(Date.now());
  }

  onMount(() => {
    // Focus the new-task dialog when the page opens via Cmd+T → command palette
    // pre-selects "Create a task" which navigates here and sets the flag.
  });
</script>

<div class="mx-auto w-full max-w-6xl" data-testid="tasks-page">
  <div class="flex items-end justify-between">
    <div>
      <h1 class="text-2xl font-semibold tracking-tight" data-testid="tasks-header">{title}</h1>
      <p class="mt-1 text-sm text-muted-foreground" data-testid="tasks-subtitle">{subtitle}</p>
    </div>
  </div>

  <div class="mt-5">
    <TaskList onSelectTask={handleSelectTask} />
  </div>
</div>

<!-- Mobile FAB (operator). Desktop uses the top-right button in TaskList. -->
<TasksFab onclick={handleFabClick} />

{#if selectedTaskId}
  <TaskDetail taskId={selectedTaskId} open={detailOpen} onclose={handleCloseDetail} />
{/if}
