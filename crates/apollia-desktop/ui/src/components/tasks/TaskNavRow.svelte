<script lang="ts">
  /**
   * TaskNavRow - a task entry in the detail-view sidebar.
   *
   * A `ListRow` (nav variant) with a status-coloured rail, the task title, an
   * agent + relative-time line, and a compact status badge. Selection state is
   * driven by the parent.
   */
  import { t } from "svelte-i18n";
  import { ListRow } from "$lib/components/operator";
  import { Badge } from "$lib/components/ui/badge";
  import { formatRelativeTime } from "$lib/utils";
  import type { TaskSummary } from "$lib/types";
  import { statusMeta } from "./taskStatus";

  interface Props {
    task: TaskSummary;
    active: boolean;
    title: string;
    onselect: (id: string) => void;
  }

  let { task, active, title, onselect }: Props = $props();

  const meta = $derived(statusMeta(task.status));
</script>

<ListRow
  variant="nav"
  state={active ? "active" : "default"}
  class="mb-0.5 text-left"
  onclick={() => onselect(task.id)}
  data-testid="tasks-split-row-{task.id}"
>
  <div class="w-0.5 self-stretch rounded-sm shrink-0 my-0.5" style="background: {meta.rail};"></div>
  <div class="min-w-0 flex-1">
    <div class="text-body-sm truncate text-foreground" style:font-weight={active ? 600 : 500}>
      {title}
    </div>
    <div class="text-body-xs text-muted-foreground mt-0.5 truncate flex items-center gap-1">
      <span>{task.agent_name || task.agent_id}</span>
      <span class="opacity-60">·</span>
      <span>{formatRelativeTime(task.created_at)}</span>
    </div>
  </div>
  <Badge size="sm" variant={meta.variant} class="shrink-0">
    {$t(meta.i18nKey)}
  </Badge>
</ListRow>
