<script lang="ts">
  /**
   * Disclosure - controlled show/hide with a summary header.
   *
   * Generalizes the chat ActivityStrip's disclosure pattern: a native
   * `<details>` grid-rows animation snaps in WKWebView, so the body is a
   * plain `{#if}` block with Svelte `slide` (gated by `rm()`), which eases
   * smoothly. The chevron rotates and `aria-expanded` tracks state.
   *
   * `controlled` re-syncs the internal state to the `open` prop on every
   * change, so a parent can drive it (e.g. open while live, collapse on
   * finalize); left off, `open` is only the initial seed and the user's
   * clicks own the state afterwards.
   */
  import type { Snippet } from "svelte";
  import { slide } from "svelte/transition";
  import { quintOut } from "svelte/easing";
  import { ChevronRight } from "lucide-svelte";
  import { duration as motionDuration, rm } from "$lib/design/motion";

  interface Props {
    /** Initial open state; also the driver when `controlled` is set. */
    open?: boolean;
    /** When true, re-sync to `open` on every change (parent-driven). */
    controlled?: boolean;
    /** Live/streaming context: sets `aria-busy` on the body. */
    live?: boolean;
    /** Slide duration in ms. Defaults to `--motion-slow` (320ms). */
    duration?: number;
    /** Header content, rendered inside the toggle button. */
    summary: Snippet;
    /** Disclosed body content. */
    children: Snippet;
    /** Optional trailing header slot (right-aligned, before the chevron). */
    meta?: Snippet;
    /** Optional `data-testid` forwarded to the root. */
    testid?: string;
  }

  let {
    open = false,
    controlled = false,
    live = false,
    duration = motionDuration.slow,
    summary,
    children,
    meta,
    testid,
  }: Props = $props();

  // Seed from `open`; when `controlled`, the effect below keeps re-syncing.
  // svelte-ignore state_referenced_locally
  let expanded = $state(open);

  $effect(() => {
    if (controlled) expanded = open;
  });

  function toggle(): void {
    expanded = !expanded;
  }
</script>

<div
  class="w-full overflow-hidden rounded-lg border border-[hsl(var(--border-soft))] bg-surface-1/60"
  data-testid={testid}
>
  <button
    type="button"
    class="flex w-full cursor-pointer select-none items-center gap-2.5 px-3 py-2
      text-left text-body-sm text-muted-foreground outline-none transition-colors
      hover:bg-surface-2/50"
    aria-expanded={expanded}
    onclick={toggle}
    data-testid={testid ? `${testid}-toggle` : undefined}
  >
    <span class="min-w-0 flex-1">
      {@render summary()}
    </span>
    {#if meta}
      <span class="flex-none">{@render meta()}</span>
    {/if}
    <ChevronRight
      size={14}
      class="ml-auto flex-none text-muted-foreground/60 transition-transform duration-base"
      style={expanded ? "transform: rotate(90deg);" : ""}
      aria-hidden="true"
    />
  </button>
  {#if expanded}
    <div
      class="tb-strip-rule px-3 pb-3 pt-2"
      aria-busy={live}
      transition:slide={rm({ duration, easing: quintOut })}
    >
      {@render children()}
    </div>
  {/if}
</div>
