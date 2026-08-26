<script lang="ts">
  import { t } from "svelte-i18n";
  import { AlertTriangle } from "lucide-svelte";
  import type { WorkspaceSnapshotView } from "$lib/types";
  import { Skeleton } from "$lib/components/ui/skeleton";

  interface Props {
    snapshot: WorkspaceSnapshotView | null;
    loading: boolean;
  }

  let { snapshot, loading }: Props = $props();
</script>

<div class="space-y-3">
  {#if loading}
    <div class="space-y-2">
      <Skeleton class="h-14 rounded-lg bg-surface-1" />
      <Skeleton class="h-14 rounded-lg bg-surface-1" />
    </div>
  {:else if snapshot}
    {#if snapshot.error_count > 0}
      <div class="flex items-center gap-2 px-3 py-2 rounded-md bg-warning/10 text-warning-a11y border border-warning/20 text-caption-lg">
        <AlertTriangle size={12} />
        {$t("projects.context_snapshot_errors", { values: { count: snapshot.error_count } })}
      </div>
    {/if}
    {#if snapshot.sections.length === 0}
      <div class="px-3 py-3 rounded-md bg-surface-1 border border-border text-caption-lg text-muted-foreground">
        {$t("projects.context_snapshot_empty")}
      </div>
    {:else}
      <ul class="space-y-2 list-none p-0 m-0">
        {#each snapshot.sections as section, i (i)}
          <li class="rounded-lg border border-border bg-surface-1 overflow-hidden">
            <div class="flex items-center gap-2 px-3 py-1.5 border-b border-border/70 bg-card">
              <span
                class="text-micro-sm font-mono uppercase tracking-[1px] px-1.5 py-px rounded border border-border text-muted-foreground"
              >
                {section.source}
              </span>
              <span class="text-caption-lg font-semibold text-foreground truncate">
                {section.title}
              </span>
            </div>
            <pre
              class="m-0 px-3 py-2 text-caption font-mono leading-[1.5] text-foreground/90 whitespace-pre-wrap overflow-auto max-h-[260px]"
            >{section.content}</pre>
          </li>
        {/each}
      </ul>
    {/if}
  {/if}
</div>
