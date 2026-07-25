<script lang="ts">
  /**
   * Quiet activity strip - zone 1 of the assistant turn.
   *
   * A single subtle summary line (spark glyph + "Reasoned and consulted N
   * sources" + duration + chevron) that collapses the agent's reasoning and
   * tool trace by default. During a live turn it is expanded so the user can
   * watch it work; once the turn finalizes it stays collapsed. The trace body
   * (reasoning captions + compact tool rows) is supplied by the caller as the
   * default snippet.
   */

  import type { Snippet } from "svelte";
  import { t, locale } from "svelte-i18n";
  import { Sparkles, ChevronRight } from "lucide-svelte";
  import { slide } from "svelte/transition";
  import { quintOut } from "svelte/easing";

  interface Props {
    /** Trace body: reasoning captions + compact tool rows. */
    children: Snippet;
    /** Expanded on mount. Live turns pass `true`; finalized turns stay closed. */
    open?: boolean;
    /** Live streaming turn: shows a working label, hides counts and duration. */
    live?: boolean;
    /** At least one thinking fragment is present in the trace. */
    hasThinking?: boolean;
    /** Deduplicated web source count (see `extractSources`). */
    sourceCount?: number;
    /** Non-pending tool call count, used when no web sources ran. */
    toolCount?: number;
    /** Summed tool duration in ms; 0 omits the figure gracefully. */
    durationMs?: number;
  }

  let {
    children,
    open = false,
    live = false,
    hasThinking = false,
    sourceCount = 0,
    toolCount = 0,
    durationMs = 0,
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

  // Bold lead verb + muted rest, mirroring the mockup's summary hierarchy.
  const summary = $derived.by<{ lead: string; rest: string }>(() => {
    if (live) {
      return { lead: $t("chat.activity.live_lead"), rest: "" };
    }
    if (sourceCount > 0) {
      return hasThinking
        ? {
            lead: $t("chat.activity.lead_reflected"),
            rest: $t("chat.activity.rest_and_sources", {
              values: { n: sourceCount },
            }),
          }
        : {
            lead: $t("chat.activity.lead_consulted"),
            rest: $t("chat.activity.rest_sources", {
              values: { n: sourceCount },
            }),
          };
    }
    if (toolCount > 0) {
      return hasThinking
        ? {
            lead: $t("chat.activity.lead_reflected"),
            rest: $t("chat.activity.rest_and_tools", {
              values: { n: toolCount },
            }),
          }
        : {
            lead: $t("chat.activity.lead_used"),
            rest: $t("chat.activity.rest_tools", { values: { n: toolCount } }),
          };
    }
    return { lead: $t("chat.activity.lead_reflected"), rest: "" };
  });

  // Locale-aware seconds, one decimal (e.g. "3,2" in fr, "3.2" in en).
  const durationLabel = $derived.by<string>(() => {
    if (live || durationMs <= 0) return "";
    const seconds = durationMs / 1000;
    const loc = $locale ?? "en";
    try {
      return new Intl.NumberFormat(loc, {
        minimumFractionDigits: 1,
        maximumFractionDigits: 1,
      }).format(seconds);
    } catch {
      return seconds.toFixed(1);
    }
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
  class="activity-strip mb-3 w-full overflow-hidden rounded-lg border border-border/60 bg-surface-1/50"
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
      <span class="font-semibold text-foreground">{summary.lead}</span><span
        >{summary.rest ? ` ${summary.rest}` : ""}</span
      >
    </span>
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
