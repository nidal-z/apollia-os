<script lang="ts">
  /**
   * SidebarBadge — counter badge for a nav entry (US-SP42-079).
   *
   * Unified variants + required symbol + required aria-label so badges
   * are never color-only. Extracted as a leaf component so its reactive
   * scope is isolated (a count change for one route does not re-render
   * the entire sidebar).
   */
  import { Badge } from "$lib/components/ui/badge";
  import { Check, Zap, Circle } from "lucide-svelte";

  type Tone = "urgent" | "attention" | "info";

  interface Props {
    count: number;
    tone?: Tone;
    /** Full sentence read by screen readers, e.g. "5 pending approvals, urgent". */
    ariaLabel: string;
    pulse?: boolean;
    testId?: string;
  }

  let {
    count,
    tone = "info",
    ariaLabel,
    pulse = false,
    testId,
  }: Props = $props();

  const variantFor: Record<Tone, "danger" | "warning" | "info"> = {
    urgent: "danger",
    attention: "warning",
    info: "info",
  };
</script>

{#if count > 0}
  <Badge
    variant={variantFor[tone]}
    size="sm"
    class="ml-auto gap-1 tabular-nums {pulse ? 'animate-pulse' : ''}"
    data-testid={testId}
    aria-label={ariaLabel}
    role="status"
  >
    <span aria-hidden="true" class="inline-flex shrink-0 items-center">
      {#if tone === "urgent"}
        <Zap size={10} strokeWidth={2.25} />
      {:else if tone === "attention"}
        <Check size={10} strokeWidth={2.25} />
      {:else}
        <Circle size={6} strokeWidth={0} fill="currentColor" />
      {/if}
    </span>
    <span>{count}</span>
  </Badge>
{/if}
