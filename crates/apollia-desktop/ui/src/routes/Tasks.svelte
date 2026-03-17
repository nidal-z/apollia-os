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

<div class="space-y-4">
  <div class="space-y-1">
    <h1 class="text-2xl font-bold" data-testid="tasks-header">{$t('tasks.title')}</h1>
    <p class="text-sm text-muted-foreground" data-testid="tasks-subtitle">{$t('tasks.subtitle')}</p>
  </div>
  <TaskList onSelectTask={handleSelectTask} />
</div>

{#if selectedTaskId}
  <TaskDetail taskId={selectedTaskId} open={detailOpen} onclose={handleCloseDetail} />
{/if}
