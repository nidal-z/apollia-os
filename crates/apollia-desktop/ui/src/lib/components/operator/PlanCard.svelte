<script lang="ts">
  // Plan card shell: the backbone surface for every plan-mode state.
  //
  // Renders the header (gradient pico tile, title, step-count / revision meta,
  // phase pill) and a body slot. The `focal` variant carries expressive depth
  // (gradient hairline, layered primary shadow, ring) and an entrance rise; it
  // marks the awaiting-approval focal moment. All colour comes from design
  // tokens, no raw value. The signature gradient is reserved for the pico tile
  // and, in the approval bar, the Approve button.
  import type { Snippet } from "svelte";
  import { fly } from "svelte/transition";
  import { t } from "svelte-i18n";
  import { ListChecks, Check, TriangleAlert } from "lucide-svelte";
  import { duration, rm } from "$lib/design/motion";

  type Tone = "default" | "success" | "destructive";
  type PhaseTone = "draft" | "awaiting" | "exec" | "done";

  interface Props {
    /** Focal card (awaiting approval): expressive depth + entrance rise. */
    focal?: boolean;
    /** Card accent tone, drives the pico tile and border. */
    tone?: Tone;
    /** Card title. */
    title: string;
    /** Step count for the meta line; omitted hides it. */
    stepCount?: number | null;
    /** Revision number for the meta line; omitted hides it. */
    revision?: number | null;
    /** Extra meta text appended after step count / revision. */
    metaExtra?: string | null;
    /** i18n key for the phase pill label; omitted hides the pill. */
    phaseKey?: string | null;
    /** Phase pill tone. */
    phaseTone?: PhaseTone;
    /** Body content. */
    children: Snippet;
    /** Optional data-testid forwarded to the root section. */
    testid?: string;
  }

  let {
    focal = false,
    tone = "default",
    title,
    stepCount = null,
    revision = null,
    metaExtra = null,
    phaseKey = null,
    phaseTone = "awaiting",
    children,
    testid,
  }: Props = $props();

  const picoTone = $derived(
    tone === "success"
      ? "border-success/30 bg-success/10 text-success"
      : tone === "destructive"
        ? "border-destructive/30 bg-destructive/10 text-destructive"
        : "border-primary/25 bg-primary/10 text-primary",
  );

  const borderTone = $derived(
    tone === "success"
      ? "border-success/35"
      : tone === "destructive"
        ? "border-destructive/35"
        : focal
          ? "border-primary/40"
          : "border-border",
  );

  const phasePillTone = $derived(
    phaseTone === "exec"
      ? "border-warning/35 bg-warning/10 text-warning"
      : phaseTone === "done"
        ? "border-success/35 bg-success/10 text-success"
        : phaseTone === "draft"
          ? "border-border bg-surface-2 text-muted-foreground"
          : "border-primary/30 bg-primary/10 text-primary",
  );

  const hasMeta = $derived(
    stepCount !== null || revision !== null || metaExtra !== null,
  );
</script>

<section
  class="relative overflow-hidden rounded-lg border bg-card text-card-foreground {borderTone} {focal
    ? 'shadow-primary-xl'
    : 'shadow-elev-1'}"
  data-testid={testid}
  in:fly={rm({ y: 10, duration: duration.slow })}
>
  {#if focal && tone === "default"}
    <span
      class="pointer-events-none absolute inset-x-0 top-0 z-10 h-0.5 bg-primary-gradient"
      aria-hidden="true"
    ></span>
  {/if}

  <header
    class="relative z-0 flex items-start gap-3 border-b border-border/60 px-4 py-3"
  >
    <span
      class="grid h-8 w-8 flex-none place-items-center rounded-md border {picoTone}"
      aria-hidden="true"
    >
      {#if tone === "success"}
        <Check size={16} />
      {:else if tone === "destructive"}
        <TriangleAlert size={16} />
      {:else}
        <ListChecks size={16} />
      {/if}
    </span>
    <div class="min-w-0 flex-1">
      <h2 class="truncate text-heading-sm text-foreground">{title}</h2>
      {#if hasMeta}
        <div
          class="mt-0.5 flex flex-wrap items-center gap-x-2 gap-y-0.5 text-caption text-muted-foreground"
        >
          {#if stepCount !== null}
            <span>{$t("plan_mode.step_count", { values: { count: stepCount } })}</span>
          {/if}
          {#if revision !== null}
            {#if stepCount !== null}
              <span class="text-muted-foreground/50" aria-hidden="true">·</span>
            {/if}
            <span>{$t("plan_mode.revision_label", { values: { n: revision } })}</span>
          {/if}
          {#if metaExtra}
            <span class="text-muted-foreground/50" aria-hidden="true">·</span>
            <span>{metaExtra}</span>
          {/if}
        </div>
      {/if}
    </div>
    {#if phaseKey}
      <span
        class="inline-flex flex-none items-center gap-1.5 rounded-full border px-2.5 py-0.5 text-overline uppercase {phasePillTone}"
      >
        <span
          class="h-1.5 w-1.5 flex-none rounded-full bg-current {phaseTone === 'exec'
            ? 'animate-pulse'
            : ''}"
          aria-hidden="true"
        ></span>
        {$t(phaseKey)}
      </span>
    {/if}
  </header>

  <div class="relative z-0 px-4 py-3">
    {@render children()}
  </div>
</section>
