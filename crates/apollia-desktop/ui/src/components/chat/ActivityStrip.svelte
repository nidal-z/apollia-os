<script lang="ts">
  /**
   * Quiet reasoning strip - zone 1 of the assistant turn.
   *
   * A single subtle summary line (spark glyph + "Reasoned" + duration +
   * chevron) that collapses the agent's reasoning (thoughts only) by default.
   * During a live turn it is expanded so the user can watch it think; once the
   * turn finalizes it stays collapsed. The trace body (flat reasoning captions)
   * is supplied by the caller as the default snippet. Tool calls and sources
   * are no longer part of the strip: they render as their own visible rows and
   * cards in the thread flow.
   */

  import type { Snippet } from "svelte";
  import { t, locale } from "svelte-i18n";
  import { Sparkles, ChevronRight } from "lucide-svelte";
  import { slide } from "svelte/transition";
  import { quintOut } from "svelte/easing";
  import { formatDurationSeconds } from "$lib/chat/duration";

  interface Props {
    /** Trace body: flat reasoning captions. */
    children: Snippet;
    /** Expanded on mount. Live turns pass `true`; finalized turns stay closed. */
    open?: boolean;
    /** Live streaming turn: shows a working label, hides the duration. */
    live?: boolean;
    /** Summed tool duration in ms; 0 omits the figure gracefully. */
    durationMs?: number;
    /** Reasoning caption count; 0 omits the figure. */
    reasoningCount?: number;
    /** Tool-call count for the turn; 0 omits the figure. */
    toolCount?: number;
  }

  let {
    children,
    open = false,
    live = false,
    durationMs = 0,
    reasoningCount = 0,
    toolCount = 0,
  }: Props = $props();

  // Controlled disclosure: the caller's `open` prop seeds the state and keeps
  // driving it (live turns start open, then collapse on finalize), while a click
  // lets the user override until the next prop change. A controlled panel plus
  // Svelte `slide` gives a smooth height animation in WKWebView, where the
  // native <details> grid-rows trick snaps instead of easing.
  let expanded = $state(open);
  $effect(() => {
    expanded = open;
  });

  function toggle(): void {
    expanded = !expanded;
  }

  // Reasoning-only summary: a single lead verb ("Reasoned" / "Working"). The
  // per-tool and per-source counts moved out of the strip with the rows and
  // cards themselves.
  const summaryLead = $derived(
    live ? $t("chat.activity.live_lead") : $t("chat.activity.lead_reflected"),
  );

  // Locale-aware seconds, one decimal (e.g. "3,2" in fr, "3.2" in en).
  const durationLabel = $derived.by<string>(() => {
    if (live || durationMs <= 0) return "";
    return formatDurationSeconds(durationMs, $locale ?? "en");
  });
</script>

<!-- Shared brand-signature gradient def. SVG url() refs resolve document-
     globally, so this single def also feeds any gradient-stroked glyph in the
     trace below. Identical duplicate defs across strips are harmless. -->
<svg
  class="pointer-events-none absolute h-0 w-0"
  aria-hidden="true"
  focusable="false"
>
  <defs>
    <linearGradient id="apollia-grad" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0" stop-color="hsl(var(--grad-a))" />
      <stop offset="1" stop-color="hsl(var(--grad-b))" />
    </linearGradient>
  </defs>
</svg>

<div
  class="activity-strip mb-4 w-full overflow-hidden rounded-lg border border-[hsl(var(--border-soft))] bg-surface-1/60"
  data-testid="activity-strip"
>
  <button
    type="button"
    class="flex w-full cursor-pointer select-none items-center gap-2.5 px-3 py-2
      text-left text-[12.5px] text-muted-foreground outline-none transition-colors
      hover:bg-surface-2/50 focus-visible:ring-2 focus-visible:ring-primary focus-visible:ring-inset"
    aria-expanded={expanded}
    aria-label={$t("chat.activity.toggle")}
    onclick={toggle}
  >
    <span class="tb-spark" aria-hidden="true">
      <Sparkles size={14} />
    </span>
    <span class="min-w-0">
      <span class="font-semibold text-foreground">{summaryLead}</span>
    </span>
    {#if !live && reasoningCount > 0}
      <span class="flex-none text-muted-foreground/70" data-testid="activity-reasoning-count"
        >· {$t("chat.activity.reasoning_count", { values: { n: reasoningCount } })}</span
      >
    {/if}
    {#if !live && toolCount > 0}
      <span class="flex-none text-muted-foreground/70" data-testid="activity-tool-count"
        >· {$t("chat.activity.tool_count", { values: { n: toolCount } })}</span
      >
    {/if}
    {#if durationLabel}
      <span
        class="flex-none tabular-nums text-muted-foreground/60"
        data-testid="activity-duration">· {durationLabel} s</span
      >
    {/if}
    <ChevronRight
      size={14}
      class="chev ml-auto flex-none text-muted-foreground/60 transition-transform duration-200"
      style={expanded ? "transform: rotate(90deg);" : ""}
      aria-hidden="true"
    />
  </button>
  {#if expanded}
    <div
      class="tb-strip-rule px-3 pb-3 pt-2"
      data-testid="activity-trace"
      transition:slide={{ duration: 280, easing: quintOut }}
    >
      {@render children()}
    </div>
  {/if}
</div>
