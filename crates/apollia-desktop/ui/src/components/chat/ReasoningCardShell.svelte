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
     * of place — those use the standard sans-serif type ramp instead.
     */
    monoTitle?: boolean;
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
  }: Props = $props();

  const titleClass = $derived(
    monoTitle
      ? "min-w-0 flex-1 truncate font-medium font-mono text-[12px] text-foreground"
      : "min-w-0 flex-1 truncate text-[12px] font-medium text-foreground/85",
  );

  const borderClass = $derived.by(() => {
    switch (status) {
      case "success":
      case "approved":
        return "border-l-2 border-l-success";
      case "error":
      case "rejected":
        return "border-l-2 border-l-destructive";
      case "pending":
      case "running":
        return "border-l-2 border-l-primary/50";
      default:
        return "";
    }
  });

  const iconBgClass = $derived.by(() => {
    switch (status) {
      case "success":
      case "approved":
        return "bg-success/10";
      case "error":
      case "rejected":
        return "bg-destructive/10";
      default:
        return "glass-inset";
    }
  });
</script>

<div
  class="my-1 rounded-lg glass-surface glass-border-subtle px-3 py-2 text-xs {borderClass}"
  data-testid={testid}
>
  <!-- Header -->
  {#if collapsible}
    <button
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
    </button>
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
