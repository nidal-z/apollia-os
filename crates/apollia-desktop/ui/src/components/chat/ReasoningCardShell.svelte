<script lang="ts">
  /**
   * Shared shell used by `ReasoningCard` and the upcoming `ApprovalCardV2`.
   * Provides the uniform header/body/footer frame + status-driven left border.
   *
   * Slots:
   *   - `icon`   : the leading icon (required)
   *   - `title`  : header title (required)
   *   - `meta`   : right-aligned header meta (optional)
   *   - `body`   : main body content (optional)
   *   - `footer` : footer actions / secondary info (optional)
   */

  import type { Snippet } from "svelte";
  import type { ReasoningStatus } from "$lib/chat/reasoning";
  import { ChevronRight } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";

  /**
   * Accent tone for the left status bar + icon background. When omitted it is
   * derived from `status`. Action cards (approval, ask-user) pass it explicitly
   * so they keep their warning/info identity regardless of internal status.
   */
  type Accent =
    | "neutral"
    | "primary"
    | "warning"
    | "info"
    | "success"
    | "destructive";

  interface Props {
    status: ReasoningStatus;
    testid?: string;
    collapsible?: boolean;
    expanded?: boolean;
    onToggle?: () => void;
    ariaLabel?: string;
    icon: Snippet;
    title: Snippet;
    meta?: Snippet;
    body?: Snippet;
    footer?: Snippet;
    /**
     * Render the title in monospace. Default `true` for tool-call cards
     * where the title is a function name. Pass `false` for narrative
     * variants (thinking, rationale) where mono looks technical and out
     * of place - those use the standard sans-serif type ramp instead.
     */
    monoTitle?: boolean;
    /** Explicit accent override; falls back to the status-derived tone. */
    accent?: Accent;
  }

  let {
    status,
    testid,
    collapsible = false,
    expanded = true,
    onToggle,
    ariaLabel,
    icon,
    title,
    meta,
    body,
    footer,
    monoTitle = true,
    accent,
  }: Props = $props();

  const titleClass = $derived(
    monoTitle
      ? "min-w-0 flex-1 truncate font-medium font-mono text-[12px] text-foreground"
      : "min-w-0 flex-1 truncate text-[12px] font-medium text-foreground/85",
  );

  const tone = $derived.by<Accent>(() => {
    if (accent) return accent;
    switch (status) {
      case "success":
      case "approved":
        return "success";
      case "error":
      case "rejected":
        return "destructive";
      case "pending":
      case "running":
        return "primary";
      default:
        return "neutral";
    }
  });

  const borderClass = $derived(
    {
      neutral: "border-l-border",
      primary: "border-l-primary",
      warning: "border-l-warning",
      info: "border-l-info",
      success: "border-l-success",
      destructive: "border-l-destructive",
    }[tone],
  );

  const iconBgClass = $derived(
    {
      neutral: "glass-inset",
      primary: "bg-primary/10",
      warning: "bg-warning/10",
      info: "bg-info/10",
      success: "bg-success/10",
      destructive: "bg-destructive/10",
    }[tone],
  );
</script>

<div
  class="my-1 rounded-lg bg-surface-1 border border-border/60 border-l-2 {borderClass} px-3 py-2 text-xs"
  data-testid={testid}
>
  <!-- Header -->
  {#if collapsible}
    <Button variant="ghost" size="sm"
      type="button"
      class="flex w-full items-center gap-1.5 text-left"
      aria-expanded={expanded}
      aria-label={ariaLabel}
      onclick={onToggle}
    >
      <span
        class="flex h-5 w-5 flex-shrink-0 items-center justify-center rounded-md {iconBgClass}"
      >
        {@render icon()}
      </span>
      <span class={titleClass}>
        {@render title()}
      </span>
      {#if meta}
        <span class="ml-auto flex flex-shrink-0 items-center gap-1">
          {@render meta()}
        </span>
      {/if}
      <ChevronRight
        size={12}
        class="flex-shrink-0 text-muted-foreground/50 transition-transform duration-150 {meta ? '' : 'ml-auto'} {expanded ? 'rotate-90' : ''}"
      />
    </Button>
  {:else}
    <div class="flex items-center gap-1.5">
      <span
        class="flex h-5 w-5 flex-shrink-0 items-center justify-center rounded-md {iconBgClass}"
      >
        {@render icon()}
      </span>
      <span class={titleClass}>
        {@render title()}
      </span>
      {#if meta}
        <span class="ml-auto flex flex-shrink-0 items-center gap-1">
          {@render meta()}
        </span>
      {/if}
    </div>
  {/if}

  {#if body && expanded}
    <div class="mt-1.5">
      {@render body()}
    </div>
  {/if}

  {#if footer && expanded}
    <div class="mt-1.5">
      {@render footer()}
    </div>
  {/if}
</div>
