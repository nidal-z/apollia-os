<script lang="ts">
  import { t } from "svelte-i18n";
  import TaskList from "../components/tasks/TaskList.svelte";
  import TaskDetail from "../components/tasks/TaskDetail.svelte";

  let selectedTaskId = $state<string | null>(null);
  let detailOpen = $state(false);

  function handleSelectTask(taskId: string) {
    selectedTaskId = taskId;
    detailOpen = true;
  }

  function handleCloseDetail() {
    detailOpen = false;
  }
</script>

<div class="mx-auto w-full max-w-6xl" data-testid="tasks-page">
  <div class="flex items-end justify-between">
    <div>
      <h1 class="text-2xl font-semibold tracking-tight" data-testid="tasks-header">{$t('tasks.title')}</h1>
      <p class="mt-1 text-sm text-muted-foreground" data-testid="tasks-subtitle">{$t('tasks.subtitle')}</p>
    </div>
  </div>

  <div class="mt-5">
    <TaskList onSelectTask={handleSelectTask} />
  </div>
</div>

{#if selectedTaskId}
  <TaskDetail taskId={selectedTaskId} open={detailOpen} onclose={handleCloseDetail} />
{/if}
