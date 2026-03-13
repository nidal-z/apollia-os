<script lang="ts">
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
  <h1 class="text-2xl font-bold">Tasks</h1>
  <TaskList onSelectTask={handleSelectTask} />
</div>

{#if selectedTaskId}
  <TaskDetail taskId={selectedTaskId} open={detailOpen} onclose={handleCloseDetail} />
{/if}
