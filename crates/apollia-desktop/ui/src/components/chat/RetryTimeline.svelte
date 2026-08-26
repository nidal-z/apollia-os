<script lang="ts">
  /**
   * Inline retry timeline.
   *
   * Horizontal mini-lane where each attempt is a colored segment:
   * - green = success
   * - red   = failed
   * - amber = timed_out
   * - blue  = fallback
   *
   * Two skins:
   * - `operator`: compact row with a retry icon and the attempt count.
   * - `builder`:  detailed lane with per-attempt tooltip and reason.
   */

  import type { RetryAttempt } from "$lib/types";
  import { RotateCcw } from "lucide-svelte";
  import { t } from "svelte-i18n";

  interface Props {
    attempts: RetryAttempt[];
    skin?: "builder" | "operator";
  }

  let { attempts, skin = "builder" }: Props = $props();

  const visible = $derived(attempts.filter((a) => !!a));
  const retryCount = $derived(Math.max(0, visible.length - 1));

  function segmentClass(a: RetryAttempt): string {
    switch (a.outcome.kind) {
      case "success":
        return "bg-success";
      case "timed_out":
        return "bg-warning";
      case "fallback":
        return "bg-info";
      case "failed":
      default:
        return "bg-destructive";
    }
  }

  function segmentLabel(a: RetryAttempt): string {
    const reason = a.reason?.human_message ?? a.reason?.technical_details ?? "";
    switch (a.outcome.kind) {
      case "success":
        return `#${a.attempt_number} · ${$t("chat.retry.success")}`;
      case "failed":
        return `#${a.attempt_number} · ${$t("chat.retry.failed")}${reason ? ` · ${reason}` : ""}`;
      case "timed_out":
        return `#${a.attempt_number} · ${$t("chat.retry.timed_out")}${reason ? ` · ${reason}` : ""}`;
      case "fallback":
        return `#${a.attempt_number} · ${$t("chat.retry.fallback")} → ${a.outcome.to}`;
    }
  }

  function durationMs(a: RetryAttempt): number {
    return Math.max(0, a.ended_at - a.started_at);
  }
</script>

{#if visible.length > 0}
  {#if skin === "operator"}
    <span
      class="inline-flex items-center gap-1 rounded-full bg-muted/40 px-1.5 py-0.5 text-micro text-muted-foreground"
      data-testid="retry-badge"
      title={$t("chat.retry.badge_title", {
        values: { n: retryCount },
      })}
    >
      <RotateCcw class="h-3 w-3" />
      {#if retryCount > 0}
        <span>{retryCount}x</span>
      {/if}
    </span>
  {:else}
    <div
      class="mt-1 rounded border border-border/30 bg-muted/20 px-2 py-1.5"
      data-testid="retry-timeline"
    >
      <div class="flex items-center gap-1.5 text-micro text-muted-foreground">
        <RotateCcw class="h-3 w-3" />
        <span>
          {$t("chat.retry.timeline_label", {
            values: { n: visible.length },
          })}
        </span>
      </div>
      <ol
        class="mt-1 flex h-1.5 w-full gap-0.5"
        aria-label={$t("chat.retry_timeline_aria")}
      >
        {#each visible as a (a.attempt_number)}
          <li
            class="h-full flex-1 rounded-sm {segmentClass(a)}"
            title={segmentLabel(a)}
          ></li>
        {/each}
      </ol>
      <ul class="mt-1 space-y-0.5 text-micro text-muted-foreground">
        {#each visible as a (a.attempt_number)}
          <li class="flex items-center gap-2">
            <span class="font-mono opacity-70">#{a.attempt_number}</span>
            <span class="flex-1 truncate">{segmentLabel(a)}</span>
            <span class="opacity-60">{durationMs(a)}ms</span>
          </li>
        {/each}
      </ul>
    </div>
  {/if}
{/if}
