<script lang="ts">
  import { t } from "svelte-i18n";
  import {
    AlertTriangle,
    Brain,
    Clock,
    FileQuestion,
    FileX,
    Lock,
    WifiOff,
    Sparkles,
    HelpCircle,
    ChevronDown,
    ChevronRight,
  } from "lucide-svelte";
  import type { ErrorAnalysis, ErrorCategory } from "$lib/types";
  import HallucinationBadge from "./HallucinationBadge.svelte";

  interface Props {
    analysis: ErrorAnalysis;
    /** Optional handler for the "suggested action" button. */
    onAction?: (action: string) => void;
  }

  let { analysis, onAction }: Props = $props();
  let detailsOpen = $state(false);

  const ICONS: Record<ErrorCategory, typeof AlertTriangle> = {
    tool_failure: AlertTriangle,
    llm_error: Brain,
    timeout: Clock,
    null_output: FileQuestion,
    malformed_output: FileX,
    permission_denied: Lock,
    network_error: WifiOff,
    hallucination_suspected: Sparkles,
    unknown: HelpCircle,
  };

  const Icon = $derived(ICONS[analysis.category] ?? HelpCircle);
  const titleKey = $derived(`error.${analysis.category}.title`);
  const titleFallback = $derived(
    {
      tool_failure: "Tool failed",
      llm_error: "AI backend error",
      timeout: "Operation timed out",
      null_output: "No output produced",
      malformed_output: "Malformed output",
      permission_denied: "Permission denied",
      network_error: "Network error",
      hallucination_suspected: "Hallucination suspected",
      unknown: "Something went wrong",
    }[analysis.category]
  );
</script>

<article class="error-card" data-category={analysis.category} data-testid="error-card">
  <header class="head">
    <span class="icon" aria-hidden="true">
      <Icon size={18} />
    </span>
    <h4 class="title">
      {$t(titleKey, { default: titleFallback })}
    </h4>
    {#if analysis.hallucination_suspected}
      <HallucinationBadge />
    {/if}
  </header>

  <p class="message">{analysis.human_message}</p>

  {#if analysis.suggested_action}
    <button
      type="button"
      class="action"
      onclick={() => onAction?.(analysis.suggested_action!)}
      data-testid="error-card-action"
    >
      {analysis.suggested_action}
    </button>
  {/if}

  <button
    type="button"
    class="details-toggle"
    aria-expanded={detailsOpen}
    onclick={() => (detailsOpen = !detailsOpen)}
  >
    {#if detailsOpen}<ChevronDown size={12} />{:else}<ChevronRight size={12} />{/if}
    {$t("error.details_toggle", { default: "Technical details" })}
  </button>

  {#if detailsOpen}
    <pre class="details" data-testid="error-card-details">{analysis.technical_details}</pre>
  {/if}
</article>

<style>
  .error-card {
    border: 1px solid hsl(var(--destructive) / 0.35);
    background-color: hsl(var(--destructive) / 0.05);
    border-radius: 0.5rem;
    padding: 0.75rem 0.875rem;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    color: hsl(var(--foreground));
  }

  .error-card[data-category="hallucination_suspected"] {
    border-color: hsl(var(--destructive) / 0.6);
    background-color: hsl(var(--destructive) / 0.1);
  }

  .head {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .icon {
    display: inline-flex;
    color: hsl(var(--destructive));
  }

  .title {
    flex: 1;
    margin: 0;
    font-size: 13px;
    font-weight: 600;
    line-height: 1.3;
  }

  .message {
    margin: 0;
    font-size: 13px;
    line-height: 1.5;
    color: hsl(var(--foreground) / 0.9);
  }

  .action {
    align-self: flex-start;
    padding: 0.3rem 0.6rem;
    border-radius: 0.375rem;
    border: 1px solid hsl(var(--border));
    background: hsl(var(--background));
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
    transition: background-color 0.15s ease;
  }

  .action:hover {
    background-color: hsl(var(--muted));
  }

  .details-toggle {
    display: inline-flex;
    align-items: center;
    gap: 0.25rem;
    background: transparent;
    border: none;
    color: hsl(var(--muted-foreground));
    font-size: 11px;
    cursor: pointer;
    padding: 0;
    align-self: flex-start;
  }

  .details {
    margin: 0;
    padding: 0.5rem;
    background: hsl(var(--muted) / 0.5);
    border-radius: 0.375rem;
    font-family: var(--font-mono);
    font-size: 11px;
    white-space: pre-wrap;
    word-break: break-word;
    max-height: 16rem;
    overflow: auto;
  }
</style>
