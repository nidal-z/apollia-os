<script lang="ts">
  // Audit row dedicated to a single plan mutation.
  // No IPC: receives the already-mapped entry and computes its diff locally.
  // Reuses the plan-review diff grammar (added / modified / removed) for color.
  import { t } from "svelte-i18n";
  import {
    PlusCircle,
    Pencil,
    Trash2,
    ArrowUpDown,
    CircleDot,
    Send,
    CheckCircle2,
    XCircle,
    ListPlus,
  } from "lucide-svelte";
  import type { ComponentType } from "svelte";
  import type { PlanMutationEntry } from "$lib/ipc/audit";
  import type { PlanMutationKind } from "$lib/ipc/plan";
  import { computePlanFieldDiff, type PlanFieldDiff } from "$lib/ipc/auditPlanRow";

  interface Props {
    entry: PlanMutationEntry;
  }

  let { entry }: Props = $props();

  const ICONS: Record<PlanMutationKind, ComponentType> = {
    propose: ListPlus,
    add_step: PlusCircle,
    modify_step: Pencil,
    remove_step: Trash2,
    reorder: ArrowUpDown,
    status_change: CircleDot,
    submit: Send,
    approve: CheckCircle2,
    reject: XCircle,
  };

  const KindIcon = $derived(ICONS[entry.kind]);
  const kindLabel = $derived($t(`observability.plan_mutation.kind.${entry.kind}`));
  const fieldDiff = $derived(computePlanFieldDiff(entry));
  const hasDiff = $derived(fieldDiff.length > 0);
  const stepTitle = $derived(
    entry.after?.title ||
      entry.before?.title ||
      entry.after?.description ||
      entry.before?.description ||
      entry.step_id ||
      "",
  );

  /** Diff color token by status, mirroring the plan-review diff grammar. */
  function valueClass(status: PlanFieldDiff["status"]): string {
    if (status === "added") return "text-success";
    if (status === "removed") return "text-destructive line-through";
    return "text-warning";
  }

  function fieldLabel(field: PlanFieldDiff["field"]): string {
    return $t(`observability.plan_mutation.field.${field}`);
  }
</script>

<tr
  class="border-b border-border/30 last:border-0"
  data-testid="plan-mutation-row-{entry.ordinal}"
>
  <td class="px-5 py-2.5 text-caption text-muted-foreground tabular-nums whitespace-nowrap align-top">
    {entry.ts ? new Date(entry.ts).toLocaleString() : ""}
  </td>
  <td class="px-3 py-2.5 align-top" colspan={5}>
    <div class="flex items-start gap-2.5">
      <span class="mt-0.5 flex-shrink-0 text-primary">
        <KindIcon class="h-4 w-4" aria-label={kindLabel} />
      </span>
      <div class="min-w-0 space-y-1.5">
        <div class="flex flex-wrap items-baseline gap-x-2 gap-y-0.5">
          <span class="text-body-xs font-semibold">{kindLabel}</span>
          {#if stepTitle}
            <span class="text-body-xs text-foreground">{stepTitle}</span>
          {/if}
          {#if entry.step_id}
            <code class="font-mono text-caption text-muted-foreground">{entry.step_id}</code>
          {/if}
          <span class="text-caption text-muted-foreground/70">
            {$t("observability.plan_mutation.revision", { values: { n: entry.revision } })}
          </span>
        </div>

        {#if entry.reason}
          <p
            class="text-caption text-muted-foreground"
            data-testid="plan-mutation-reason-{entry.ordinal}"
          >
            <span class="font-medium">{$t("observability.plan_mutation.reason")}</span>:
            {entry.reason}
          </p>
        {/if}

        {#if hasDiff}
          <dl
            class="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-caption"
            data-testid="plan-mutation-diff-{entry.ordinal}"
          >
            {#each fieldDiff as diff (diff.field)}
              <dt class="section-meta pt-0.5">
                {fieldLabel(diff.field)}
              </dt>
              <dd class="m-0 flex flex-wrap items-baseline gap-x-2 gap-y-0.5 font-mono">
                {#if diff.status === "modified"}
                  <del class="text-destructive line-through">{diff.before}</del>
                  <span aria-hidden="true" class="text-muted-foreground/60">&rarr;</span>
                  <ins class="text-warning no-underline">{diff.after}</ins>
                {:else if diff.status === "added"}
                  <ins class={`no-underline ${valueClass("added")}`}>+ {diff.after}</ins>
                {:else}
                  <del class={valueClass("removed")}>- {diff.before}</del>
                {/if}
              </dd>
            {/each}
          </dl>
        {/if}
      </div>
    </div>
  </td>
</tr>
