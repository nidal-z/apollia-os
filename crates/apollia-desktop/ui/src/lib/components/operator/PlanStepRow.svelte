<script lang="ts">
  // One plan step row.
  //
  // Echoes the chat `.tb-steps` / `.tb-step` idiom: a leading status glyph, the
  // step title and a trailing status pill (`.tb-pill` tones), extended with an
  // optional description, a dependency line and a Builder detail slot. Status
  // colour comes exclusively from `stepStatusToken`; the numbered badge morphs
  // between states via a colour transition, never a hard swap.
  import type { Snippet } from "svelte";
  import { t } from "svelte-i18n";
  import { Check, X } from "lucide-svelte";
  import type { StepStatus } from "$lib/ipc/plan";
  import { stepStatusToken } from "./stepStatusToken";

  interface Props {
    /** 1-based position shown in the badge. */
    index: number;
    /** Primary step label. */
    title: string;
    /** Optional secondary description. */
    description?: string | null;
    /** Resolved dependency label, e.g. "#2" or "s1a3f"; omitted hides the line. */
    deps?: string | null;
    /** Step status, drives glyph and pill. Defaults to pending. */
    status?: StepStatus;
    /** Replan tombstone: dropped step, faded + struck through. */
    tomb?: boolean;
    /** Highlights the current (in-progress) row with a primary wash. */
    current?: boolean;
    /** Optional Builder detail (chips, rationale) rendered under the body. */
    detail?: Snippet;
    /** Optional data-testid on the row. */
    testid?: string;
  }

  let {
    index,
    title,
    description = null,
    deps = null,
    status = "pending",
    tomb = false,
    current = false,
    detail,
    testid,
  }: Props = $props();

  const tokens = $derived(stepStatusToken(status));

  // Pill tone reusing the chat `.tb-pill` modifiers.
  const pillClass = $derived(
    status === "in_progress"
      ? "tb-active"
      : status === "completed"
        ? "tb-ok"
        : status === "failed"
          ? "tb-error"
          : "",
  );

  const badgeClass = $derived(
    status === "completed"
      ? "bg-success text-success-foreground"
      : status === "in_progress"
        ? "bg-primary text-primary-foreground ring-4 ring-primary/15"
        : status === "failed"
          ? "bg-destructive text-destructive-foreground"
          : status === "skipped"
            ? "bg-surface-2 text-muted-foreground"
            : "bg-primary/10 text-primary",
  );
</script>

<li
  class="flex items-start gap-2.5 rounded-md border border-transparent px-2 py-2 transition-colors hover:bg-surface-2/60 {current
    ? 'border-primary/25 bg-primary/5'
    : ''} {tomb ? 'opacity-55' : ''}"
  data-testid={testid}
  data-status={status}
>
  <span
    class="mt-px grid h-5 w-5 flex-none place-items-center rounded-full text-overline font-bold tabular-nums transition-colors {badgeClass} {status ===
    'in_progress'
      ? 'animate-pulse'
      : ''}"
    aria-hidden="true"
  >
    {#if status === "completed"}
      <Check size={12} strokeWidth={3} />
    {:else if status === "failed"}
      <X size={12} strokeWidth={3} />
    {:else}
      {index}
    {/if}
  </span>

  <div class="min-w-0 flex-1">
    <p
      class="text-body-sm font-medium leading-snug text-foreground {status ===
      'completed'
        ? 'text-muted-foreground'
        : ''} {tomb || status === 'skipped' ? 'line-through' : ''}"
    >
      {title}
    </p>
    {#if description}
      <p class="mt-0.5 text-body-xs leading-snug text-muted-foreground">
        {description}
      </p>
    {/if}
    {#if deps}
      <p class="mt-1 text-caption text-muted-foreground/80">
        <span class="font-semibold">{$t("plan_mode.step_dependencies_label")}</span>:
        {deps}
      </p>
    {/if}
    {#if detail}
      {@render detail()}
    {/if}
  </div>

  <span class="tb-pill tb-step-pill {pillClass}">{$t(tokens.labelKey)}</span>
</li>
