<script lang="ts">
  /**
   * TaskListRow - a single task row for the list view.
   *
   * Wraps the shared operator `TaskRow` in a right-click `ContextMenu` whose
   * items are derived from the task status (open / retry / cancel), and forwards
   * a left-click to open the detail. Row enter / leave / reorder motion is owned
   * by the parent `{#each}` (listMotion presets), so this component stays thin.
   */
  import { t } from "svelte-i18n";
  import { Eye, RefreshCw, CircleSlash, Trash2 } from "lucide-svelte";
  import { TaskRow, type Task as DSTask } from "$lib/components/operator";
  import { ContextMenu, type ActionMenuItem } from "$lib/components/ui/context-menu";
  import type { TaskSummary } from "$lib/types";

  interface Props {
    task: TaskSummary;
    row: DSTask;
    onselect: (id: string) => void;
    onretry: (task: TaskSummary) => void;
    oncancel: (id: string) => void;
    ondelete: (task: TaskSummary) => void;
  }

  let { task, row, onselect, onretry, oncancel, ondelete }: Props = $props();

  const isActive = $derived(
    task.status === "working" ||
      task.status === "submitted" ||
      task.status === "input_required",
  );
  const isRetryable = $derived(task.status === "failed" || task.status === "canceled");
  // Delete removes the record; it is only offered once the task has settled.
  // A live task (working / submitted / awaiting approval) must be cancelled, not deleted.
  const isDeletable = $derived(!isActive);

  // Pressing Delete / Backspace while a settled row is focused requests removal.
  function onRowKeydown(event: KeyboardEvent) {
    if (event.key !== "Delete" && event.key !== "Backspace") return;
    if (!isDeletable) return;
    event.preventDefault();
    ondelete(task);
  }

  const menuItems = $derived.by<ActionMenuItem[]>(() => {
    const items: ActionMenuItem[] = [
      {
        id: "open",
        label: $t("tasks.ctx_run"),
        icon: Eye,
        onclick: () => onselect(task.id),
        testid: `task-ctx-open-${task.id}`,
      },
    ];
    if (isRetryable) {
      items.push({
        id: "retry",
        label: $t("tasks.ctx_retry"),
        icon: RefreshCw,
        onclick: () => onretry(task),
        testid: `task-ctx-retry-${task.id}`,
      });
    }
    if (isActive) {
      items.push({
        id: "cancel",
        label: $t("tasks.ctx_cancel"),
        icon: CircleSlash,
        variant: "destructive",
        onclick: () => oncancel(task.id),
        testid: `task-ctx-cancel-${task.id}`,
      });
    }
    if (isDeletable) {
      items.push({
        id: "delete",
        label: $t("tasks.ctx_delete"),
        icon: Trash2,
        variant: "destructive",
        onclick: () => ondelete(task),
        testid: `task-ctx-delete-${task.id}`,
      });
    }
    return items;
  });
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div onkeydown={onRowKeydown}>
  <ContextMenu items={menuItems} data-testid="task-ctx-menu-{task.id}">
    <TaskRow task={row} onclick={() => onselect(task.id)} />
  </ContextMenu>
</div>
