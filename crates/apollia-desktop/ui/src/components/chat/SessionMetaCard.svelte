<script lang="ts">
  import { t } from "svelte-i18n";
  import { invoke } from "@tauri-apps/api/core";
  import { Sparkles, ArrowRight } from "lucide-svelte";
  import type { SessionMeta, SessionHallucinationInputs } from "$lib/types";
  import HallucinationRiskBadge from "./HallucinationRiskBadge.svelte";

  interface Props {
    session_id: string;
    /** Signaux agrégés côté store (P3 flags, citation gaps, contradictions). */
    signals: SessionHallucinationInputs;
    /** Callback when the user clicks a "Next step" chip — injects the prompt. */
    onNextStepClick?: (step: string) => void;
  }

  let { session_id, signals, onNextStepClick }: Props = $props();

  let meta = $state<SessionMeta | null>(null);
  let loading = $state(false);
  let error = $state<string | null>(null);

  async function refresh() {
    loading = true;
    error = null;
    try {
      meta = await invoke<SessionMeta>("compute_session_meta", {
        sessionId: session_id,
        inputs: signals,
      });
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    if (session_id) {
      void refresh();
    }
  });
</script>

<section
  class="session-meta-card"
  data-testid="session-meta-card"
  aria-label={$t("session_meta.card_label", { default: "Session summary" })}
>
  <header class="header">
    <div class="title-row">
      <Sparkles size={14} />
      <span class="title">
        {meta?.title ??
          $t("session_meta.untitled", { default: "Untitled session" })}
      </span>
    </div>
    {#if meta}
      <HallucinationRiskBadge risk={meta.hallucination_risk} />
    {/if}
  </header>

  {#if loading && !meta}
    <p class="muted">{$t("common.loading", { default: "Loading…" })}</p>
  {:else if error}
    <p class="error" data-testid="session-meta-error">{error}</p>
  {:else if meta}
    <p class="summary" data-testid="session-meta-summary">
      {meta.summary ??
        $t("session_meta.summary.fallback", {
          default:
            "Enable the session meta routine in settings to generate an AI summary.",
        })}
    </p>

    {#if meta.next_steps && meta.next_steps.length > 0}
      <div class="next-steps" data-testid="session-meta-next-steps">
        <span class="section-label">
          {$t("session_meta.next_steps.label", { default: "Next steps" })}
        </span>
        <div class="chips">
          {#each meta.next_steps as step (step)}
            <button
              type="button"
              class="chip"
              onclick={() => onNextStepClick?.(step)}
            >
              <ArrowRight size={11} />
              <span>{step}</span>
            </button>
          {/each}
        </div>
      </div>
    {/if}

    <footer class="footer">
      <span class="event-count">
        {$t("session_meta.events.count", {
          default: "{count} events",
          values: { count: meta.event_count },
        })}
      </span>
    </footer>
  {/if}
</section>

<style>
  .session-meta-card {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
    padding: 1rem 1.125rem;
    border-radius: 0.75rem;
    border: 1px solid hsl(var(--border));
    background-color: hsl(var(--card) / 0.6);
    backdrop-filter: blur(8px);
  }
  .header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 0.75rem;
  }
  .title-row {
    display: inline-flex;
    align-items: center;
    gap: 0.375rem;
    font-weight: 600;
    font-size: 13px;
  }
  .title {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .summary {
    font-size: 13px;
    line-height: 1.5;
    color: hsl(var(--muted-foreground));
  }
  .muted {
    font-size: 12px;
    color: hsl(var(--muted-foreground));
  }
  .error {
    font-size: 12px;
    color: hsl(var(--destructive));
  }
  .section-label {
    text-transform: uppercase;
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.05em;
    color: hsl(var(--muted-foreground));
    display: block;
    margin-bottom: 0.375rem;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 0.375rem;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 0.25rem;
    padding: 0.25rem 0.625rem;
    border-radius: 9999px;
    border: 1px solid hsl(var(--border));
    background-color: hsl(var(--secondary) / 0.4);
    font-size: 11px;
    font-weight: 500;
    cursor: pointer;
    transition: background-color 120ms ease;
    text-align: left;
  }
  .chip:hover {
    background-color: hsl(var(--secondary) / 0.7);
  }
  .footer {
    display: flex;
    justify-content: flex-end;
    font-size: 11px;
    color: hsl(var(--muted-foreground));
  }
  .event-count {
    font-variant-numeric: tabular-nums;
  }
</style>
